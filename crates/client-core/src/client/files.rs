//! The calls for files: adding what a prompt carries, and reading the images the transcript draws.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::client::{ClientRequest, ClientResponse};
use protocol::model::{AttachmentId, SessionId, WorkspaceId};

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

    /// See [`ClientRequest::WorkspaceImageRead`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_image_read(&self, id: &WorkspaceId, path: &str) -> Result<(MediaType, Vec<u8>)> {
        let reply = self.call(ClientRequest::WorkspaceImageRead { id: id.clone(), path: path.to_string() })?;
        let ClientResponse::Image { media_type, data } = reply else {
            return unexpected(&reply);
        };
        Ok((media_type, data.into_vec()))
    }
}
