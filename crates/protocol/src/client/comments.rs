//! The requests about the comments on a workspace's diff, waiting to be sent to its agent.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{ClientRequestId, CommentId, DiffComment, DiffSide, WorkspaceId};

/// Sending the same `client_request_id` again returns the comment it made, unless it's been deleted since, which
/// forgets the ID. The comment being written on the workspace's diff goes with it if it's on the same line, says the
/// same and changes no comment, since it's this one saved.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CommentAdd {
    pub workspace_id: WorkspaceId,
    pub path: String,
    pub side: DiffSide,
    pub line: u32,
    pub text: String,
    #[serde(default)]
    pub client_request_id: Option<ClientRequestId>,
}

impl Request for CommentAdd {
    type Reply = DiffComment;

    fn into_request(self) -> ClientRequest {
        ClientRequest::CommentAdd(self)
    }
}

impl Reply for DiffComment {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Comment(comment) = response else { return Err(response) };
        Ok(comment)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CommentList {
    pub workspace_id: WorkspaceId,
}

impl Request for CommentList {
    type Reply = Comments;

    fn into_request(self) -> ClientRequest {
        ClientRequest::CommentList(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Comments {
    pub comments: Vec<DiffComment>,
}

impl Reply for Comments {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Comments(comments) = response else { return Err(response) };
        Ok(comments)
    }
}

/// Replaces a comment's text, keeping its place in the diff. Sending it again sets the same text. The comment being
/// written goes with it if it changes this one and says the same.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CommentEdit {
    pub id: CommentId,
    pub text: String,
}

impl Request for CommentEdit {
    type Reply = DiffComment;

    fn into_request(self) -> ClientRequest {
        ClientRequest::CommentEdit(self)
    }
}

/// Deleting a comment that's already gone does nothing. The comment being written, if it changes this one, becomes a
/// new comment on the same line, so what's typed in it isn't lost.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CommentDelete {
    pub id: CommentId,
}

impl Request for CommentDelete {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::CommentDelete(self)
    }
}
