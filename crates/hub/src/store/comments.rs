//! Comments left on a diff.

use protocol::model::CommentId;
use protocol::model::DiffComment;
use protocol::model::WorkspaceId;
use rusqlite::params;

use super::{Store, StoreError, from_text, lock, text_of};

impl Store {
    pub fn add_comment(&self, comment: &DiffComment) -> Result<(), StoreError> {
        lock(&self.writer).execute(
            "INSERT INTO diff_comments (id, workspace_id, path, side, line, text, created_at_ms)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                comment.id.as_str(),
                comment.workspace_id.as_str(),
                comment.path,
                text_of(&comment.side),
                comment.line,
                comment.text,
                comment.created_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn list_comments(&self, workspace_id: &WorkspaceId) -> Result<Vec<DiffComment>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(
            "SELECT id, workspace_id, path, side, line, text, created_at_ms FROM diff_comments
                 WHERE workspace_id = ? ORDER BY created_at_ms, id",
        )?;
        let rows = statement.query_map([workspace_id.as_str()], |row| {
            Ok(DiffComment {
                id: row.get::<_, String>(0)?.into(),
                workspace_id: row.get::<_, String>(1)?.into(),
                path: row.get(2)?,
                side: from_text(row.get(3)?)?,
                line: row.get(4)?,
                text: row.get(5)?,
                created_at_ms: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    pub fn delete_comment(&self, id: &CommentId) -> Result<(), StoreError> {
        lock(&self.writer).execute("DELETE FROM diff_comments WHERE id = ?", [id.as_str()])?;
        Ok(())
    }
}
