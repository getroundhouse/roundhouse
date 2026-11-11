//! The calls for files: adding what a prompt carries, reading the images the transcript draws, and browsing a
//! worktree's files.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::client::{ClientRequest, ClientResponse};
use protocol::model::{AttachmentId, CommitId, FileContents, SessionId, WorkspaceId, WorktreeFiles};

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
        Ok(attachment)
    }

    pub fn attachment_read(&self, session_id: &SessionId, id: &AttachmentId) -> Result<Vec<u8>> {
        let reply = self.call(ClientRequest::AttachmentRead { session_id: session_id.clone(), id: id.clone() })?;
        let ClientResponse::AttachmentData { data } = reply else {
            return unexpected(&reply);
        };
        Ok(data.into_vec())
    }

    /// See [`ClientRequest::WorkspaceFiles`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_files(&self, id: &WorkspaceId) -> Result<WorktreeFiles> {
        let reply = self.call(ClientRequest::WorkspaceFiles { id: id.clone() })?;
        let ClientResponse::WorktreeFiles(files) = reply else {
            return unexpected(&reply);
        };
        Ok(files)
    }

    /// See [`ClientRequest::WorkspaceFolderRead`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_folder_read(&self, id: &WorkspaceId, path: &str) -> Result<WorktreeFiles> {
        let reply = self.call(ClientRequest::WorkspaceFolderRead { id: id.clone(), path: path.to_string() })?;
        let ClientResponse::WorktreeFiles(files) = reply else {
            return unexpected(&reply);
        };
        Ok(files)
    }

    /// See [`ClientRequest::WorkspaceFileRead`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_file_read(&self, id: &WorkspaceId, path: &str) -> Result<FileContents> {
        let reply = self.call(ClientRequest::WorkspaceFileRead { id: id.clone(), path: path.to_string() })?;
        let ClientResponse::FileContents(contents) = reply else {
            return unexpected(&reply);
        };
        Ok(contents)
    }

    /// What a file held in `commit`, such as the diff's base, to show what a change replaced. A hub older than this
    /// refuses it, as `is_unsupported` says.
    pub fn workspace_file_read_at(&self, id: &WorkspaceId, commit: &CommitId, path: &str) -> Result<FileContents> {
        let request =
            ClientRequest::WorkspaceFileReadAt { id: id.clone(), commit: commit.clone(), path: path.to_string() };
        let reply = self.call(request)?;
        let ClientResponse::FileContents(contents) = reply else {
            return unexpected(&reply);
        };
        Ok(contents)
    }
}
