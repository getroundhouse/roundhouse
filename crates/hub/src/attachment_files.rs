//! Where the hub keeps the bytes of the files prompts carry, and of the images an agent's tools send: a file each, at
//! `<session>/<attachment>` in the attachments directory, outside the database so copying it before a migration stays
//! quick. Each ID is checked as it's turned into a path, so nothing a client or worker sends can name a file anywhere
//! else.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AttachmentId, SessionId};

use crate::error::internal;

/// Longer than any ID a client or worker makes, which is a UUID.
const MAX_ID_CHARS: usize = 64;

pub(crate) struct AttachmentFiles {
    dir: PathBuf,
    /// Makes each write's partial file its own, so two writes of the same attachment can't mix.
    writes: AtomicU64,
}

impl AttachmentFiles {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir, writes: AtomicU64::new(0) }
    }

    fn path(&self, session_id: &SessionId, id: &AttachmentId) -> Result<PathBuf, ApiError> {
        Ok(self.session_dir(session_id)?.join(safe(id.as_str())?))
    }

    fn session_dir(&self, session_id: &SessionId) -> Result<PathBuf, ApiError> {
        Ok(self.dir.join(safe(session_id.as_str())?))
    }

    /// Refuses an ID that couldn't be kept, before anything is written.
    pub(crate) fn check(&self, session_id: &SessionId, id: &AttachmentId) -> Result<(), ApiError> {
        self.path(session_id, id).map(drop)
    }

    /// Written under a name of its own and then moved into place, so a reader never finds half of it.
    pub(crate) async fn write(&self, session_id: &SessionId, id: &AttachmentId, data: Vec<u8>) -> Result<(), ApiError> {
        let path = self.path(session_id, id)?;
        let partial = path.with_extension(format!("{}.partial", self.writes.fetch_add(1, Ordering::Relaxed)));
        on_disk(move || write_whole(&path, &partial, &data))
            .await
            .map_err(|e| internal(format!("couldn't keep an attachment: {e}")))
    }

    /// `None` if the hub doesn't have its bytes, either because it never did or because it let them go.
    pub(crate) async fn read(&self, session_id: &SessionId, id: &AttachmentId) -> Result<Option<Vec<u8>>, ApiError> {
        let path = self.path(session_id, id)?;
        match on_disk(move || std::fs::read(path)).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(internal(format!("couldn't read an attachment: {e}"))),
        }
    }

    pub(crate) async fn remove(&self, session_id: &SessionId, id: &AttachmentId) -> Result<(), ApiError> {
        let path = self.path(session_id, id)?;
        match on_disk(move || std::fs::remove_file(path)).await {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                Err(internal(format!("couldn't remove an attachment: {e}")))
            }
            _ => Ok(()),
        }
    }

    /// Every file a session had, once it's been deleted.
    pub(crate) async fn remove_session(&self, session_id: &SessionId) -> Result<(), ApiError> {
        let dir = self.session_dir(session_id)?;
        match on_disk(move || std::fs::remove_dir_all(dir)).await {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                Err(internal(format!("couldn't remove a deleted session's attachments: {e}")))
            }
            _ => Ok(()),
        }
    }
}

/// Only lowercase letters, digits, `-` and `_`, which a UUID is, so no ID can climb out of the directory, and no two
/// name one file on a disk that ignores case, as a Mac's does.
fn safe(id: &str) -> Result<&str, ApiError> {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
    if id.is_empty() || id.len() > MAX_ID_CHARS || !id.chars().all(allowed) {
        let message = "an attachment's ID can only be lowercase letters, digits, - and _";
        return Err(ApiError::new(ErrorCode::InvalidRequest, message));
    }
    Ok(id)
}

/// Off the async threads, since a file can be large.
async fn on_disk<T: Send + 'static>(f: impl FnOnce() -> io::Result<T> + Send + 'static) -> io::Result<T> {
    tokio::task::spawn_blocking(f).await.unwrap_or_else(|e| Err(io::Error::other(e)))
}

fn write_whole(path: &Path, partial: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(partial, data)?;
    let moved = std::fs::rename(partial, path);
    if moved.is_err() {
        let _ = std::fs::remove_file(partial);
    }
    moved
}

#[cfg(test)]
mod tests;
