//! The calls for files: adding what a prompt carries, reading the images the transcript draws and the videos it
//! plays, and browsing a worktree's files.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::client::{attachments, files};
use protocol::model::{
    AttachmentId, CommitId, FileContents, FileOpened, FilePages, FileRange, SessionId, WorkspaceId, WorktreeFiles,
};

use super::{Client, Result};

impl Client {
    /// Adding the same `id` again returns what the first one added.
    pub fn attachment_add(
        &self,
        session_id: &SessionId,
        id: &AttachmentId,
        name: &str,
        media_type: MediaType,
        data: Vec<u8>,
    ) -> Result<Attachment> {
        let attachment = self.call(attachments::AttachmentAdd {
            session_id: session_id.clone(),
            id: id.clone(),
            name: name.to_string(),
            media_type,
            data: AttachmentBytes::new(data),
        })?;
        self.inner.given.lock().insert((session_id.clone(), id.clone()));
        Ok(attachment)
    }

    /// Whether this connection has added the file to the session, or read it back from it, so the hub has it, unless
    /// it has let go of it since.
    pub(crate) fn has_given(&self, session_id: &SessionId, id: &AttachmentId) -> bool {
        self.inner.given.lock().contains(&(session_id.clone(), id.clone()))
    }

    /// Takes back what [`Self::has_given`] says, for a file the hub turned out not to have.
    pub(crate) fn forget_given(&self, session_id: &SessionId, id: &AttachmentId) {
        self.inner.given.lock().remove(&(session_id.clone(), id.clone()));
    }

    pub fn attachment_read(&self, session_id: &SessionId, id: &AttachmentId) -> Result<Vec<u8>> {
        let attachments::AttachmentData { data } =
            self.call(attachments::AttachmentRead { session_id: session_id.clone(), id: id.clone() })?;
        self.inner.given.lock().insert((session_id.clone(), id.clone()));
        Ok(data.into_vec())
    }

    /// See [`attachments::AttachmentRemove`].
    pub fn attachment_remove(&self, session_id: &SessionId, id: &AttachmentId) -> Result<()> {
        self.call(attachments::AttachmentRemove { session_id: session_id.clone(), id: id.clone() })?;
        self.forget_given(session_id, id);
        Ok(())
    }

    /// See [`files::WorkspaceFiles`].
    pub fn workspace_files(&self, id: &WorkspaceId) -> Result<WorktreeFiles> {
        self.call(files::WorkspaceFiles { id: id.clone() })
    }

    /// See [`files::WorkspaceFolderRead`].
    pub fn workspace_folder_read(&self, id: &WorkspaceId, path: &str) -> Result<WorktreeFiles> {
        self.call(files::WorkspaceFolderRead { id: id.clone(), path: path.to_string() })
    }

    /// See [`attachments::AttachmentReadRange`].
    pub fn attachment_read_range(
        &self,
        session_id: &SessionId,
        id: &AttachmentId,
        start: u64,
        length: u32,
    ) -> Result<FileRange> {
        self.call(attachments::AttachmentReadRange { session_id: session_id.clone(), id: id.clone(), start, length })
    }

    /// See [`files::WorkspaceFileReadRange`]. A hub whose worker is older than this refuses it, as `is_unsupported`
    /// says.
    pub fn workspace_file_read_range(
        &self,
        id: &WorkspaceId,
        path: &str,
        start: u64,
        length: u32,
    ) -> Result<FileRange> {
        self.call(files::WorkspaceFileReadRange { id: id.clone(), path: path.to_string(), start, length })
    }

    /// See [`files::WorkspaceFileRead`].
    pub fn workspace_file_read(&self, id: &WorkspaceId, path: &str) -> Result<FileContents> {
        self.call(files::WorkspaceFileRead { id: id.clone(), path: path.to_string() })
    }

    /// What a file held in `commit`, such as the diff's base, to show what a change replaced.
    pub fn workspace_file_read_at(&self, id: &WorkspaceId, commit: &CommitId, path: &str) -> Result<FileContents> {
        self.call(files::WorkspaceFileReadAt { id: id.clone(), commit: commit.clone(), path: path.to_string() })
    }

    /// Opens a file as it was in `commit`, to be read a page at a time with [`Self::workspace_file_pages`], for showing
    /// one that can be too big for [`Self::workspace_file_read_at`] to send.
    pub fn workspace_file_open_at(&self, id: &WorkspaceId, commit: &CommitId, path: &str) -> Result<FileOpened> {
        self.call(files::WorkspaceFileOpenAt { id: id.clone(), commit: commit.clone(), path: path.to_string() })
    }

    /// Pages of a file opened with [`Self::workspace_file_open_at`], with as many lines a page as the open said, as
    /// many as fit in a reply, so what's asked for and not sent is asked for again.
    pub fn workspace_file_pages(
        &self,
        id: &WorkspaceId,
        commit: &CommitId,
        path: &str,
        page_lines: u32,
        pages: &[u32],
    ) -> Result<FilePages> {
        let (id, commit, path, pages) = (id.clone(), commit.clone(), path.to_string(), pages.to_vec());
        self.call(files::WorkspaceFilePages { id, commit, path, page_lines, pages })
    }
}
