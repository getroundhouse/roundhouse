//! The calls that keep what's typed and not sent: a chat's draft, an answer being typed to its question, and a comment
//! being written on a workspace's diff. `Drafts` makes them, on a thread of its own.

use protocol::client::{ClientRequest, drafts};
use protocol::model::{AnswerDraft, CommentDraft, NewDraft, Session, SessionId, Workspace, WorkspaceId};

use super::{Client, Result};

impl Client {
    /// See [`drafts::SessionDraft`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn session_draft(&self, id: &SessionId, draft: &NewDraft) -> Result<Session> {
        self.expect_session(ClientRequest::SessionDraft(drafts::SessionDraft { id: id.clone(), draft: draft.clone() }))
    }

    /// See [`drafts::SessionAnswerDraft`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn session_answer_draft(&self, id: &SessionId, draft: Option<&AnswerDraft>) -> Result<Session> {
        self.expect_session(ClientRequest::SessionAnswerDraft(drafts::SessionAnswerDraft {
            id: id.clone(),
            draft: draft.cloned(),
        }))
    }

    /// See [`drafts::WorkspaceCommentDraft`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_comment_draft(&self, id: &WorkspaceId, draft: Option<&CommentDraft>) -> Result<Workspace> {
        self.expect_workspace(ClientRequest::WorkspaceCommentDraft(drafts::WorkspaceCommentDraft {
            id: id.clone(),
            draft: draft.cloned(),
        }))
    }
}
