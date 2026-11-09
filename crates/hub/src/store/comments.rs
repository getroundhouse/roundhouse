//! Comments left on a diff.

use protocol::model::{ClientRequestId, CommentId, DiffComment, Rev, WorkspaceId};
use rusqlite::{Row, params};

use super::made::{Made, inserted, made_by};
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
    pub fn add_comment(
        &self,
        comment: &DiffComment,
        client_request_id: Option<&ClientRequestId>,
    ) -> Result<Made<DiffComment>, StoreError> {
        let conn = lock(&self.writer);
        if let Some(made) = made_by(&conn, client_request_id)? {
            return Ok(Made::Earlier(made));
        }
        conn.execute(
            &format!(
                "INSERT INTO diff_comments ({COMMENT_COLUMNS}, client_request_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            ),
            params![
                comment.id.as_str(),
                comment.workspace_id.as_str(),
                comment.path,
                text_of(&comment.side),
                comment.line,
                comment.text,
                comment.created_at_ms,
                client_request_id.map(ClientRequestId::as_str),
            ],
        )?;
        Ok(inserted(&conn, comment.id.as_str())?)
    }

    pub fn list_comments(&self, workspace_id: &WorkspaceId) -> Result<Vec<DiffComment>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {COMMENT_COLUMNS}, rev FROM diff_comments WHERE workspace_id = ? ORDER BY created_at_ms, id"
        ))?;
        let rows = statement.query_map([workspace_id.as_str()], comment_from_row)?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    /// Replaces a comment's text, and returns it as stored, with its new revision.
    pub fn edit_comment(&self, id: &CommentId, text: &str) -> Result<DiffComment, StoreError> {
        let conn = lock(&self.writer);
        let changed = conn.execute("UPDATE diff_comments SET text = ? WHERE id = ?", params![text, id.as_str()])?;
        if changed == 0 {
            return Err(StoreError::NotFound { kind: "comment", id: id.to_string() });
        }
        let query = format!("SELECT {COMMENT_COLUMNS}, rev FROM diff_comments WHERE id = ?");
        Ok(conn.query_row(&query, [id.as_str()], comment_from_row)?)
    }

    /// Returns the revision of the removal, or `None` if there was nothing to remove.
    pub fn delete_comment(&self, id: &CommentId) -> Result<Option<Rev>, StoreError> {
        self.delete("diff_comments", id.as_str())
    }
}
