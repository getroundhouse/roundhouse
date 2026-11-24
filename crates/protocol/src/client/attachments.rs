//! The requests about a session's files: adding one for a prompt to carry, reading one back, and letting go of one
//! that's no longer going anywhere.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::attachment::{Attachment, AttachmentBytes, MediaType};
use crate::model::{AttachmentId, SessionId};

/// Adds a file to a session, for a prompt to carry. Sending the same `id` again returns the attachment it made,
/// whatever else it says. Refused if the file is too big, or isn't the kind it says, as [`MediaType::check`] decides.
/// A hub refuses a kind it doesn't know, such as one newer than it, as unsupported, so a client finds out before it
/// sends the prompt, and can add the file as [`MediaType::Other`] instead.
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
/// added to has it, and once its prompt has gone, the hub keeps only images made of pixels, so any other file, a
/// drawing such as an SVG included, is not found. A video an agent's text points to is refused with `InvalidRequest`,
/// since it's read a part at a time, with [`AttachmentReadRange`], as it plays.
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

/// Part of a file the hub keeps, `length` bytes from `start`, for playing a video an agent's text points to, which
/// the hub keeps as an attachment. Not found as `AttachmentRead`'s file is, and refused with `InvalidRequest` when
/// `length` is 0 or more than [`crate::model::MAX_RANGE_BYTES`]. It answers with a [`crate::model::FileRange`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AttachmentReadRange {
    pub session_id: SessionId,
    pub id: AttachmentId,
    pub start: u64,
    pub length: u32,
}

impl Request for AttachmentReadRange {
    type Reply = crate::model::FileRange;

    fn into_request(self) -> ClientRequest {
        ClientRequest::AttachmentReadRange(self)
    }
}

/// Lets go of a file a client added to a session that's no longer going anywhere, such as one taken off a draft. The
/// hub keeps one the session's draft, a prompt waiting in its queue or a prompt it was sent still names, and one put in
/// the worktree, and it never lets go of an image from the agent. One that isn't there changes nothing, so sending it
/// again does nothing more. Answers with `Ok` either way.
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
