//! The calls for files: adding what a prompt carries, reading the images the transcript draws, and browsing a
//! worktree's files.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::client::{ClientRequest, ClientResponse, files};
use protocol::model::{
    AttachmentId, CommitId, FileContents, FileOpened, FilePages, SessionId, WorkspaceId, WorktreeFiles,
};

use super::{Client, Result, unexpected};

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
        let reply = self.call(ClientRequest::AttachmentAdd {
            session_id: session_id.clone(),
            id: id.clone(),
            name: name.to_string(),
            media_type,
            data: AttachmentBytes::new(data),
        })?;
        let ClientResponse::Attachment(attachment) = reply else {
            return unexpected(&reply);
        };
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
        let reply = self.call(ClientRequest::AttachmentRead { session_id: session_id.clone(), id: id.clone() })?;
        let ClientResponse::AttachmentData { data } = reply else {
            return unexpected(&reply);
        };
        self.inner.given.lock().insert((session_id.clone(), id.clone()));
        Ok(data.into_vec())
    }

    /// See [`ClientRequest::AttachmentRemove`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn attachment_remove(&self, session_id: &SessionId, id: &AttachmentId) -> Result<()> {
        let reply = self.call(ClientRequest::AttachmentRemove { session_id: session_id.clone(), id: id.clone() })?;
        let ClientResponse::Ok = reply else {
            return unexpected(&reply);
        };
        self.forget_given(session_id, id);
        Ok(())
    }

    /// See [`ClientRequest::WorkspaceFiles`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_files(&self, id: &WorkspaceId) -> Result<WorktreeFiles> {
        let reply = self.call(ClientRequest::WorkspaceFiles(files::WorkspaceFiles { id: id.clone() }))?;
        let ClientResponse::WorktreeFiles(files) = reply else {
            return unexpected(&reply);
        };
        Ok(files)
    }

    /// See [`ClientRequest::WorkspaceFolderRead`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_folder_read(&self, id: &WorkspaceId, path: &str) -> Result<WorktreeFiles> {
        let reply = self.call(ClientRequest::WorkspaceFolderRead(files::WorkspaceFolderRead {
            id: id.clone(),
            path: path.to_string(),
        }))?;
        let ClientResponse::WorktreeFiles(files) = reply else {
            return unexpected(&reply);
        };
        Ok(files)
    }

    /// See [`ClientRequest::WorkspaceFileRead`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_file_read(&self, id: &WorkspaceId, path: &str) -> Result<FileContents> {
        let reply = self.call(ClientRequest::WorkspaceFileRead(files::WorkspaceFileRead {
            id: id.clone(),
            path: path.to_string(),
        }))?;
        let ClientResponse::FileContents(contents) = reply else {
            return unexpected(&reply);
        };
        Ok(contents)
    }

    /// What a file held in `commit`, such as the diff's base, to show what a change replaced. A hub older than this
    /// refuses it, as `is_unsupported` says.
    pub fn workspace_file_read_at(&self, id: &WorkspaceId, commit: &CommitId, path: &str) -> Result<FileContents> {
        let request = ClientRequest::WorkspaceFileReadAt(files::WorkspaceFileReadAt {
            id: id.clone(),
            commit: commit.clone(),
            path: path.to_string(),
        });
        let reply = self.call(request)?;
        let ClientResponse::FileContents(contents) = reply else {
            return unexpected(&reply);
        };
        Ok(contents)
    }

    /// Opens a file as it was in `commit`, to be read a page at a time with [`Self::workspace_file_pages`], for
    /// showing one that can be too big for [`Self::workspace_file_read_at`] to send. A hub older than this refuses
    /// it, as `is_unsupported` says.
    pub fn workspace_file_open_at(&self, id: &WorkspaceId, commit: &CommitId, path: &str) -> Result<FileOpened> {
        let (id, commit, path) = (id.clone(), commit.clone(), path.to_string());
        let reply = self.call(ClientRequest::WorkspaceFileOpenAt(files::WorkspaceFileOpenAt { id, commit, path }))?;
        let ClientResponse::FileOpened(opened) = reply else {
            return unexpected(&reply);
        };
        Ok(opened)
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
        let reply = self.call(ClientRequest::WorkspaceFilePages(files::WorkspaceFilePages {
            id,
            commit,
            path,
            page_lines,
            pages,
        }))?;
        let ClientResponse::FilePages(pages) = reply else {
            return unexpected(&reply);
        };
        Ok(pages)
    }
}
