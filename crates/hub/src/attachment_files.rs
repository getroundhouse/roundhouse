//! Where the hub keeps the bytes of the files prompts carry, and of the images an agent's tools send: a file each, at
//! `<session>/<attachment>` in the attachments directory, outside the database so copying it before a migration stays
//! quick. The new-workspace dialog's files wait at `workspace-draft.files/<attachment>`, a name no session's can be.
//! Each ID is checked as it's turned into a path, so nothing a client or worker sends can name a file anywhere else.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AttachmentId, SessionId};

use crate::error::internal;

/// Longer than any ID a client or worker makes, which is a UUID.
const MAX_ID_CHARS: usize = 64;

/// Where the new-workspace dialog's files wait, which has a `.`, so no session's directory can be it. It keeps the name
/// it was first made with, since a hub may have files there already.
const NEW_WORKSPACE_DRAFT_DIR: &str = "workspace-draft.files";

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

    fn draft_path(&self, id: &AttachmentId) -> Result<PathBuf, ApiError> {
        Ok(self.dir.join(NEW_WORKSPACE_DRAFT_DIR).join(safe(id.as_str())?))
    }

    /// Refuses an ID that couldn't be kept, before anything is written.
    pub(crate) fn check(&self, session_id: &SessionId, id: &AttachmentId) -> Result<(), ApiError> {
        self.path(session_id, id).map(drop)
    }

    /// The same, for one of the new-workspace dialog's files.
    pub(crate) fn check_draft_file(&self, id: &AttachmentId) -> Result<(), ApiError> {
        self.draft_path(id).map(drop)
    }

    /// Written under a name of its own and then moved into place, so a reader never finds half of it.
    pub(crate) async fn write(&self, session_id: &SessionId, id: &AttachmentId, data: Vec<u8>) -> Result<(), ApiError> {
        self.write_at(self.path(session_id, id)?, data).await
    }

    /// The same, for one of the new-workspace dialog's files.
    pub(crate) async fn write_draft_file(&self, id: &AttachmentId, data: Vec<u8>) -> Result<(), ApiError> {
        self.write_at(self.draft_path(id)?, data).await
    }

    async fn write_at(&self, path: PathBuf, data: Vec<u8>) -> Result<(), ApiError> {
        let partial = path.with_extension(format!("{}.partial", self.writes.fetch_add(1, Ordering::Relaxed)));
        on_disk(move || write_whole(&path, &partial, &data))
            .await
            .map_err(|e| internal(format!("couldn't keep an attachment: {e}")))
    }

    /// `None` if the hub doesn't have its bytes, either because it never did or because it let them go.
    pub(crate) async fn read(&self, session_id: &SessionId, id: &AttachmentId) -> Result<Option<Vec<u8>>, ApiError> {
        read_at(self.path(session_id, id)?).await
    }

    /// The same, for one of the new-workspace dialog's files.
    pub(crate) async fn read_draft_file(&self, id: &AttachmentId) -> Result<Option<Vec<u8>>, ApiError> {
        read_at(self.draft_path(id)?).await
    }

    pub(crate) async fn remove(&self, session_id: &SessionId, id: &AttachmentId) -> Result<(), ApiError> {
        remove_at(self.path(session_id, id)?).await
    }

    /// The same, for one of the new-workspace dialog's files.
    pub(crate) async fn remove_draft_file(&self, id: &AttachmentId) -> Result<(), ApiError> {
        remove_at(self.draft_path(id)?).await
    }

    /// Lets go of each of the new-workspace dialog's files on disk but those in `kept`, such as one a hub stopping
    /// mid-write left half of, or one whose row went without it.
    pub(crate) async fn remove_draft_files_except(&self, kept: HashSet<AttachmentId>) -> Result<(), ApiError> {
        let dir = self.dir.join(NEW_WORKSPACE_DRAFT_DIR);
        on_disk(move || {
            let entries = match std::fs::read_dir(&dir) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
                entries => entries?,
            };
            for entry in entries {
                let entry = entry?;
                let name = entry.file_name();
                if !name.to_str().is_some_and(|name| kept.contains(&AttachmentId::from(name))) {
                    std::fs::remove_file(entry.path())?;
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| internal(format!("couldn't let go of the new-workspace dialog's files: {e}")))
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

async fn read_at(path: PathBuf) -> Result<Option<Vec<u8>>, ApiError> {
    match on_disk(move || std::fs::read(path)).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(internal(format!("couldn't read an attachment: {e}"))),
    }
}

async fn remove_at(path: PathBuf) -> Result<(), ApiError> {
    match on_disk(move || std::fs::remove_file(path)).await {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(internal(format!("couldn't remove an attachment: {e}"))),
        _ => Ok(()),
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
