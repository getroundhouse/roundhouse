use crate::error::internal;
use protocol::attachment::AttachmentBytes;
use protocol::client::ApiError;
use protocol::model::{AttachmentId, ByteRange, FileRange, SessionId};
use protocol::paths;
use std::collections::HashSet;
use std::io;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Where the new-workspace dialog's files wait, which has a `.`, so no session's directory can be it. It keeps the name
/// it was first made with, since a hub may have files there already.
pub(super) const NEW_WORKSPACE_DRAFT_DIR: &str = "workspace-draft.files";

/// What a file is kept for, which says where it's kept: a session, for its prompts to carry, or the new-workspace
/// dialog, before any session exists to add it to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileFor {
    Session(SessionId),
    NewWorkspace,
}

pub(crate) struct AttachmentFiles {
    pub(super) dir: PathBuf,
    /// Makes each write's partial file its own, so two writes of the same attachment can't mix.
    pub(super) writes: AtomicU64,
}

impl AttachmentFiles {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir, writes: AtomicU64::new(0) }
    }

    pub(super) fn path(&self, kept_for: &FileFor, id: &AttachmentId) -> Result<PathBuf, ApiError> {
        let dir = match kept_for {
            FileFor::Session(session_id) => self.session_dir(session_id)?,
            FileFor::NewWorkspace => self.dir.join(NEW_WORKSPACE_DRAFT_DIR),
        };
        Ok(dir.join(paths::file_name_id("an attachment's", id.as_str())?))
    }

    pub(super) fn session_dir(&self, session_id: &SessionId) -> Result<PathBuf, ApiError> {
        Ok(self.dir.join(paths::file_name_id("a session's", session_id.as_str())?))
    }

    /// Refuses an ID that couldn't be kept, before anything is written.
    pub(crate) fn check(&self, kept_for: &FileFor, id: &AttachmentId) -> Result<(), ApiError> {
        self.path(kept_for, id).map(drop)
    }

    /// Written under a name of its own and then moved into place, so a reader never finds half of it.
    pub(crate) async fn write(&self, kept_for: &FileFor, id: &AttachmentId, data: Vec<u8>) -> Result<(), ApiError> {
        let path = self.path(kept_for, id)?;
        let partial = path.with_extension(format!("{}.partial", self.writes.fetch_add(1, Ordering::Relaxed)));
        on_disk(move || write_whole(&path, &partial, &data))
            .await
            .map_err(|e| internal(format!("couldn't keep an attachment: {e}")))
    }

    /// `None` if the hub doesn't have its bytes, either because it never did or because it let them go.
    pub(crate) async fn read(&self, kept_for: &FileFor, id: &AttachmentId) -> Result<Option<Vec<u8>>, ApiError> {
        let path = self.path(kept_for, id)?;
        match on_disk(move || std::fs::read(path)).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(internal(format!("couldn't read an attachment: {e}"))),
        }
    }

    /// Part of its bytes, such as a video's, which a player reads as it needs them. `None` as [`Self::read`] says.
    pub(crate) async fn read_range(
        &self,
        kept_for: &FileFor,
        id: &AttachmentId,
        range: ByteRange,
    ) -> Result<Option<FileRange>, ApiError> {
        let path = self.path(kept_for, id)?;
        match on_disk(move || read_part(&path, range)).await {
            Ok(part) => Ok(Some(part)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(internal(format!("couldn't read an attachment: {e}"))),
        }
    }

    pub(crate) async fn remove(&self, kept_for: &FileFor, id: &AttachmentId) -> Result<(), ApiError> {
        let path = self.path(kept_for, id)?;
        match on_disk(move || std::fs::remove_file(path)).await {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                Err(internal(format!("couldn't remove an attachment: {e}")))
            }
            _ => Ok(()),
        }
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

/// Off the async threads, since a file can be large.
async fn on_disk<T: Send + 'static>(f: impl FnOnce() -> io::Result<T> + Send + 'static) -> io::Result<T> {
    tokio::task::spawn_blocking(f).await.unwrap_or_else(|e| Err(io::Error::other(e)))
}

fn read_part(path: &Path, range: ByteRange) -> io::Result<FileRange> {
    let mut file = std::fs::File::open(path)?;
    let file_bytes = file.metadata()?.len();
    let within = range.within(file_bytes);
    file.seek(SeekFrom::Start(within.start))?;
    let mut data = Vec::new();
    file.take(within.end - within.start).read_to_end(&mut data)?;
    Ok(FileRange { start: range.start(), data: AttachmentBytes::new(data), file_bytes })
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
