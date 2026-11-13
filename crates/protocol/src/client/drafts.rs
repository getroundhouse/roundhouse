//! The requests that keep what's typed and not sent, so it's there after the app restarts, and in every client: in a
//! session's composer, in answer to its question, on a workspace's diff, and in the new-workspace dialog, with its
//! files. Keeping what's kept already changes nothing, so sending one again does nothing more. A hub older than one
//! refuses it, as `is_unsupported` says.

use serde::{Deserialize, Serialize};

use super::attachments::AttachmentData;
use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::attachment::{Attachment, AttachmentBytes, MediaType};
use crate::model::{
    AnswerDraft, AttachmentId, CommentDraft, DraftToKeep, NewWorkspaceDraft, NewWorkspaceDraftToKeep, Session,
    SessionId, Workspace, WorkspaceId,
};

/// Keeps `draft` as what's typed and attached in the session's composer and not sent, in place of what was kept. An
/// empty one clears it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionDraft {
    pub id: SessionId,
    pub draft: DraftToKeep,
}

impl Request for SessionDraft {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionDraft(self)
    }
}

/// Keeps `draft` as what's chosen and typed in answer to the question the session's agent is waiting on, in place of
/// what was kept. `None` clears it, as one with nothing chosen or typed does, and so does the question being answered
/// or cancelled, whoever answers it, after which one for it is left out.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionAnswerDraft {
    pub id: SessionId,
    pub draft: Option<AnswerDraft>,
}

impl Request for SessionAnswerDraft {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionAnswerDraft(self)
    }
}

/// Keeps `draft` as the comment being written on the workspace's diff, in place of what was kept. `None` clears it,
/// which a client sends once the comment is cancelled. Saving it lets go of it on the hub, as `CommentAdd` and
/// `CommentEdit` say, so a client has the hub keep what it saves before saving it, rather than clearing it first, which
/// would lose it if the save failed. One on a side of a diff the hub doesn't know is refused, as `CommentAdd` refuses
/// it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceCommentDraft {
    pub id: WorkspaceId,
    pub draft: Option<CommentDraft>,
}

impl Request for WorkspaceCommentDraft {
    type Reply = Workspace;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceCommentDraft(self)
    }
}

/// Keeps `draft` as what's in the new-workspace dialog, as [`NewWorkspaceDraft`] says, in place of what was kept.
/// Answers with the draft as kept.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewWorkspaceDraftKeep {
    pub draft: NewWorkspaceDraftToKeep,
}

impl Request for NewWorkspaceDraftKeep {
    type Reply = NewWorkspaceDraft;

    fn into_request(self) -> ClientRequest {
        ClientRequest::NewWorkspaceDraftKeep(self)
    }
}

impl Reply for NewWorkspaceDraft {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::NewWorkspaceDraft(kept) = response else { return Err(response) };
        Ok(kept)
    }
}

/// A file for the new-workspace dialog's draft to name, kept by the hub before any workspace or chat exists to add it
/// to, with the same checks as `AttachmentAdd`. Sent again with the same ID, it returns what the first one added
/// without looking at the rest.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewWorkspaceDraftFileAdd {
    pub id: AttachmentId,
    pub name: String,
    pub media_type: MediaType,
    pub data: AttachmentBytes,
}

impl Request for NewWorkspaceDraftFileAdd {
    type Reply = Attachment;

    fn into_request(self) -> ClientRequest {
        ClientRequest::NewWorkspaceDraftFileAdd(self)
    }
}

/// A file's bytes, for putting the draft's files back in the dialog, such as in another client. Not found once the hub
/// has let go of it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewWorkspaceDraftFileRead {
    pub id: AttachmentId,
}

impl Request for NewWorkspaceDraftFileRead {
    type Reply = AttachmentData;

    fn into_request(self) -> ClientRequest {
        ClientRequest::NewWorkspaceDraftFileRead(self)
    }
}

/// Lets go of a file the draft no longer names, and of its bytes. The hub keeps one the draft still names. One that
/// isn't there changes nothing. Answers with `Ok` either way.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewWorkspaceDraftFileRemove {
    pub id: AttachmentId,
}

impl Request for NewWorkspaceDraftFileRemove {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::NewWorkspaceDraftFileRemove(self)
    }
}
