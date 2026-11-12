//! The calls that keep what's typed and not sent: a chat's draft, an answer being typed to its question, a comment
//! being written on a workspace's diff, and what's in the new-workspace dialog, with its files. `Drafts` makes them, on
//! a thread of its own.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::client::{ClientRequest, ClientResponse, drafts};
use protocol::model::{
    AnswerDraft, AttachmentId, CommentDraft, NewDraft, Session, SessionId, Workspace, WorkspaceDraft,
    WorkspaceDraftToKeep, WorkspaceId,
};

use super::{Client, Result, unexpected};

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

    /// See [`drafts::WorkspaceDraftKeep`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_draft_keep(&self, draft: &WorkspaceDraftToKeep) -> Result<WorkspaceDraft> {
        let reply =
            self.call(ClientRequest::WorkspaceDraftKeep(drafts::WorkspaceDraftKeep { draft: draft.clone() }))?;
        let ClientResponse::WorkspaceDraft(kept) = reply else { return unexpected(&reply) };
        Ok(kept)
    }

    /// See [`drafts::WorkspaceDraftFileAdd`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_draft_file_add(
        &self,
        id: &AttachmentId,
        name: &str,
        media_type: MediaType,
        data: Vec<u8>,
    ) -> Result<Attachment> {
        let data = AttachmentBytes::new(data);
        let request = ClientRequest::WorkspaceDraftFileAdd(drafts::WorkspaceDraftFileAdd {
            id: id.clone(),
            name: name.to_string(),
            media_type,
            data,
        });
        let reply = self.call(request)?;
        let ClientResponse::Attachment(file) = reply else { return unexpected(&reply) };
        Ok(file)
    }

    /// See [`drafts::WorkspaceDraftFileRead`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_draft_file_read(&self, id: &AttachmentId) -> Result<Vec<u8>> {
        let reply =
            self.call(ClientRequest::WorkspaceDraftFileRead(drafts::WorkspaceDraftFileRead { id: id.clone() }))?;
        let ClientResponse::AttachmentData { data } = reply else { return unexpected(&reply) };
        Ok(data.into_vec())
    }

    /// See [`drafts::WorkspaceDraftFileRemove`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_draft_file_remove(&self, id: &AttachmentId) -> Result<()> {
        self.expect_ok(ClientRequest::WorkspaceDraftFileRemove(drafts::WorkspaceDraftFileRemove { id: id.clone() }))
    }
}
