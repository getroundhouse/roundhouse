//! The calls that keep what's typed and not sent: a chat's draft, an answer being typed to its question, a comment
//! being written on a workspace's diff, and what's in the new-workspace dialog, with its files. `Drafts` makes them, on
//! a thread of its own. And the one that makes a workspace from what's kept of the dialog.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::client::drafts;
use protocol::client::new_workspace::{NewWorkspaceCreate, NewWorkspaceMade};
use protocol::model::{
    AnswerDraft, AttachmentId, ClientRequestId, CommentDraft, DraftToKeep, NewWorkspaceDraft, NewWorkspaceDraftToKeep,
    Session, SessionId, Workspace, WorkspaceId,
};

use super::{Client, Result};

impl Client {
    /// See [`drafts::SessionDraft`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn session_draft(&self, id: &SessionId, draft: &DraftToKeep) -> Result<Session> {
        self.call(drafts::SessionDraft { id: id.clone(), draft: draft.clone() })
    }

    /// See [`drafts::SessionAnswerDraft`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn session_answer_draft(&self, id: &SessionId, draft: Option<&AnswerDraft>) -> Result<Session> {
        self.call(drafts::SessionAnswerDraft { id: id.clone(), draft: draft.cloned() })
    }

    /// See [`drafts::WorkspaceCommentDraft`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_comment_draft(&self, id: &WorkspaceId, draft: Option<&CommentDraft>) -> Result<Workspace> {
        self.call(drafts::WorkspaceCommentDraft { id: id.clone(), draft: draft.cloned() })
    }

    /// See [`drafts::NewWorkspaceDraftKeep`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn new_workspace_draft_keep(&self, draft: &NewWorkspaceDraftToKeep) -> Result<NewWorkspaceDraft> {
        self.call(drafts::NewWorkspaceDraftKeep { draft: draft.clone() })
    }

    /// See [`drafts::NewWorkspaceDraftFileAdd`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn new_workspace_draft_file_add(
        &self,
        id: &AttachmentId,
        name: &str,
        media_type: MediaType,
        data: Vec<u8>,
    ) -> Result<Attachment> {
        let data = AttachmentBytes::new(data);
        self.call(drafts::NewWorkspaceDraftFileAdd { id: id.clone(), name: name.to_string(), media_type, data })
    }

    /// See [`drafts::NewWorkspaceDraftFileRead`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn new_workspace_draft_file_read(&self, id: &AttachmentId) -> Result<Vec<u8>> {
        Ok(self.call(drafts::NewWorkspaceDraftFileRead { id: id.clone() })?.data.into_vec())
    }

    /// See [`drafts::NewWorkspaceDraftFileRemove`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn new_workspace_draft_file_remove(&self, id: &AttachmentId) -> Result<()> {
        self.call(drafts::NewWorkspaceDraftFileRemove { id: id.clone() })
    }

    /// See [`NewWorkspaceCreate`]. A hub older than this refuses it, as `is_unsupported` says, and a client then makes
    /// the workspace a step at a time, sending the files itself.
    pub fn new_workspace_create(
        &self,
        made_of: &NewWorkspaceDraftToKeep,
        client_request_id: ClientRequestId,
    ) -> Result<NewWorkspaceMade> {
        self.call(NewWorkspaceCreate { made_of: made_of.clone(), client_request_id })
    }
}
