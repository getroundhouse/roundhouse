//! Comments left on a diff.

use protocol::model::{CommentId, DiffComment, Rev, WorkspaceId};
use rusqlite::{Row, params};

use super::{Store, StoreError, from_text, lock, rev_of, text_of};

pub(super) const COMMENT_COLUMNS: &str = "id, workspace_id, path, side, line, text, created_at_ms";

pub(super) fn comment_from_row(row: &Row) -> rusqlite::Result<DiffComment> {
    Ok(DiffComment {
        id: row.get::<_, String>(0)?.into(),
        workspace_id: row.get::<_, String>(1)?.into(),
        path: row.get(2)?,
        side: from_text(row.get(3)?)?,
        line: row.get(4)?,
        text: row.get(5)?,
        created_at_ms: row.get(6)?,
        rev: rev_of(row, 7)?,
    })
}

impl Store {
    /// Returns the comment as stored, with its revision.
    pub fn add_comment(&self, comment: &DiffComment) -> Result<DiffComment, StoreError> {
        let conn = lock(&self.writer);
        conn.execute(
            &format!("INSERT INTO diff_comments ({COMMENT_COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?)"),
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
        conn.query_row(
            &format!("SELECT {COMMENT_COLUMNS}, rev FROM diff_comments WHERE id = ?"),
            [comment.id.as_str()],
            comment_from_row,
        )
        .map_err(StoreError::from)
    }

    pub fn list_comments(&self, workspace_id: &WorkspaceId) -> Result<Vec<DiffComment>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {COMMENT_COLUMNS}, rev FROM diff_comments WHERE workspace_id = ? ORDER BY created_at_ms, id"
        ))?;
        let rows = statement.query_map([workspace_id.as_str()], comment_from_row)?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    /// Returns the revision of the removal, or `None` if there was nothing to remove.
    pub fn delete_comment(&self, id: &CommentId) -> Result<Option<Rev>, StoreError> {
        self.delete("diff_comments", id.as_str())
    }
}
