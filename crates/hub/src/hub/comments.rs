//! Comments left on a diff, and the one being written there.

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{
    ClientRequestId, CommentDraft, CommentId, DiffComment, DiffSide, RecordKey, Rev, Workspace, WorkspaceId,
};

use super::{Hub, new_id};
use crate::store::{Made, now_ms};

impl Hub {
    /// Keeps the comment being written on the workspace's diff, and tells the feed if that changed it.
    pub(crate) async fn workspace_comment_draft(
        &self,
        id: WorkspaceId,
        draft: Option<CommentDraft>,
    ) -> Result<Workspace, ApiError> {
        let (workspace, changed) = self.blocking(move |store| store.set_comment_draft(&id, draft.as_ref())).await?;
        if changed {
            self.notify(HubNotice::Workspace(workspace.clone()));
        }
        Ok(workspace)
    }

    pub(crate) async fn comment_add(
        &self,
        workspace_id: WorkspaceId,
        path: String,
        side: DiffSide,
        line: u32,
        text: String,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<DiffComment, ApiError> {
        if let Some(made) = self.made_earlier(client_request_id.as_ref()).await? {
            return Ok(made);
        }
        if text.trim().is_empty() {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "a comment can't be empty"));
        }
        if side == DiffSide::Unknown {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "this hub doesn't know that side of a diff"));
        }
        self.workspace(&workspace_id).await?;
        let comment = DiffComment {
            id: new_id().into(),
            workspace_id,
            path,
            side,
            line,
            text,
            created_at_ms: now_ms(),
            rev: Rev::default(),
        };
        match self.blocking(move |store| store.add_comment(&comment, client_request_id.as_ref())).await? {
            Made::New(made) => {
                self.notify(HubNotice::Comment(made.clone()));
                Ok(made)
            }
            Made::Earlier(made) => Ok(made),
        }
    }

    pub(crate) async fn comment_list(&self, workspace_id: WorkspaceId) -> Result<Vec<DiffComment>, ApiError> {
        self.blocking(move |store| store.list_comments(&workspace_id)).await
    }

    pub(crate) async fn comment_edit(&self, id: CommentId, text: String) -> Result<DiffComment, ApiError> {
        if text.trim().is_empty() {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "a comment can't be empty"));
        }
        let edited = self.blocking(move |store| store.edit_comment(&id, &text)).await?;
        self.notify(HubNotice::Comment(edited.clone()));
        Ok(edited)
    }

    pub(crate) async fn comment_delete(&self, id: CommentId) -> Result<(), ApiError> {
        let deleted = id.clone();
        if let Some(rev) = self.blocking(move |store| store.delete_comment(&deleted)).await? {
            self.notify(HubNotice::Removed { record: RecordKey::Comment { id }, rev });
        }
        Ok(())
    }
}
