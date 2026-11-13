//! The requests about a session's files: adding one for a prompt to carry, reading one back, and letting go of one
//! that's no longer going anywhere.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::attachment::{Attachment, AttachmentBytes, MediaType};
use crate::model::{AttachmentId, SessionId};

/// Adds a file to a session, for a prompt to carry. Sending the same `id` again returns the attachment it made,
/// whatever else it says. Refused if the file is too big, or isn't the kind it says, as [`MediaType::check`] decides.
/// A hub older than attachments refuses it as unsupported, so a client finds out before it sends the prompt.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AttachmentAdd {
    pub session_id: SessionId,
    pub id: AttachmentId,
    pub name: String,
    pub media_type: MediaType,
    pub data: AttachmentBytes,
}

impl Request for AttachmentAdd {
    type Reply = Attachment;

    fn into_request(self) -> ClientRequest {
        ClientRequest::AttachmentAdd(self)
    }
}

/// What a file added is answered with: the file as the hub keeps it.
impl Reply for Attachment {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Attachment(attachment) = response else { return Err(response) };
        Ok(attachment)
    }
}

/// A file's bytes, for drawing an image, or for putting a draft's file back in the composer. Only the session it was
/// added to has it, and once its prompt has gone, the hub keeps only images, so any other file is not found.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AttachmentRead {
    pub session_id: SessionId,
    pub id: AttachmentId,
}

impl Request for AttachmentRead {
    type Reply = AttachmentData;

    fn into_request(self) -> ClientRequest {
        ClientRequest::AttachmentRead(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AttachmentData {
    pub data: AttachmentBytes,
}

impl Reply for AttachmentData {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::AttachmentData(data) = response else { return Err(response) };
        Ok(data)
    }
}

/// Lets go of a file a client added to a session that's no longer going anywhere, such as one taken off a draft. The
/// hub keeps one the session's draft, a prompt waiting in its queue or a prompt it was sent still names, and one put in
/// the worktree, and it never lets go of an image from the agent. One that isn't there changes nothing, so sending it
/// again does nothing more. Answers with `Ok` either way. A hub older than this refuses it, as `is_unsupported` says.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AttachmentRemove {
    pub session_id: SessionId,
    pub id: AttachmentId,
}

impl Request for AttachmentRemove {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::AttachmentRemove(self)
    }
}
