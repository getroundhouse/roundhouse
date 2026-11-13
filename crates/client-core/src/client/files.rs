//! The calls for files: adding what a prompt carries, reading the images the transcript draws, and browsing a
//! worktree's files.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::client::{attachments, files};
use protocol::model::{
    AttachmentId, CommitId, FileContents, FileOpened, FilePages, SessionId, WorkspaceId, WorktreeFiles,
};

use super::{Client, Result};

impl Client {
    /// Adding the same `id` again returns what the first one added. A hub older than attachments refuses this, as
    /// `is_unsupported` says, before anything is sent to the agent.
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

    /// See [`attachments::AttachmentRemove`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn attachment_remove(&self, session_id: &SessionId, id: &AttachmentId) -> Result<()> {
        self.call(attachments::AttachmentRemove { session_id: session_id.clone(), id: id.clone() })?;
        self.forget_given(session_id, id);
        Ok(())
    }

    /// See [`files::WorkspaceFiles`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_files(&self, id: &WorkspaceId) -> Result<WorktreeFiles> {
        self.call(files::WorkspaceFiles { id: id.clone() })
    }

    /// See [`files::WorkspaceFolderRead`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_folder_read(&self, id: &WorkspaceId, path: &str) -> Result<WorktreeFiles> {
        self.call(files::WorkspaceFolderRead { id: id.clone(), path: path.to_string() })
    }

    /// See [`files::WorkspaceFileRead`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_file_read(&self, id: &WorkspaceId, path: &str) -> Result<FileContents> {
        self.call(files::WorkspaceFileRead { id: id.clone(), path: path.to_string() })
    }

    /// What a file held in `commit`, such as the diff's base, to show what a change replaced. A hub older than this
    /// refuses it, as `is_unsupported` says.
    pub fn workspace_file_read_at(&self, id: &WorkspaceId, commit: &CommitId, path: &str) -> Result<FileContents> {
        self.call(files::WorkspaceFileReadAt { id: id.clone(), commit: commit.clone(), path: path.to_string() })
    }

    /// Opens a file as it was in `commit`, to be read a page at a time with [`Self::workspace_file_pages`], for
    /// showing one that can be too big for [`Self::workspace_file_read_at`] to send. A hub older than this refuses
    /// it, as `is_unsupported` says.
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
