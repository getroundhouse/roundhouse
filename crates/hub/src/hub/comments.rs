//! Comments left on a diff.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{CommentId, DiffComment, DiffSide, WorkspaceId};

use super::{Hub, new_id};
use crate::store::now_ms;

impl Hub {
    pub(crate) async fn comment_add(
        &self,
        workspace_id: WorkspaceId,
        path: String,
        side: DiffSide,
        line: u32,
        text: String,
    ) -> Result<DiffComment, ApiError> {
        if text.trim().is_empty() {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "a comment can't be empty"));
        }
        if side == DiffSide::Unknown {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "this hub doesn't know that side of a diff"));
        }
        self.workspace(&workspace_id).await?;
        let comment =
            DiffComment { id: new_id().into(), workspace_id, path, side, line, text, created_at_ms: now_ms() };
        let stored = comment.clone();
        self.blocking(move |store| store.add_comment(&stored)).await?;
        Ok(comment)
    }

    pub(crate) async fn comment_list(&self, workspace_id: WorkspaceId) -> Result<Vec<DiffComment>, ApiError> {
        self.blocking(move |store| store.list_comments(&workspace_id)).await
    }

    pub(crate) async fn comment_delete(&self, id: CommentId) -> Result<(), ApiError> {
        self.blocking(move |store| store.delete_comment(&id)).await
    }
}
