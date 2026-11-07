//! Comments left on a diff.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{DiffComment, DiffSide};

use super::{Hub, new_id};
use crate::store::now_ms;

impl Hub {
    pub(crate) async fn comment_add(
        &self,
        workspace_id: String,
        path: String,
        side: DiffSide,
        line: u32,
        text: String,
    ) -> Result<DiffComment, ApiError> {
        if text.trim().is_empty() {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "a comment can't be empty"));
        }
        self.workspace(&workspace_id).await?;
        let comment = DiffComment { id: new_id(), workspace_id, path, side, line, text, created_at_ms: now_ms() };
        let stored = comment.clone();
        self.blocking(move |store| store.add_comment(&stored)).await?;
        Ok(comment)
    }

    pub(crate) async fn comment_list(&self, workspace_id: String) -> Result<Vec<DiffComment>, ApiError> {
        self.blocking(move |store| store.list_comments(&workspace_id)).await
    }

    pub(crate) async fn comment_delete(&self, id: String) -> Result<(), ApiError> {
        self.blocking(move |store| store.delete_comment(&id)).await
    }
}
