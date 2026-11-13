//! Comments left on a diff, and what saving or deleting one does to the comment being written there.

use protocol::model::{ClientRequestId, CommentDraft, CommentId, DiffComment, Rev, Workspace, WorkspaceId};
use rusqlite::{Connection, OptionalExtension as _, Row, params};

use super::made::{Made, inserted, made_by};
use super::workspaces::workspace_on;
use super::{Store, StoreError, current_rev, from_text, lock, rev_of, text_of};

#[cfg(test)]
mod tests;

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

/// What a change to a comment made, and its workspace if the comment being written on the diff changed with it, in
/// the same commit, so the feed tells of both and no client sees one without the other.
#[derive(Debug, PartialEq)]
pub struct Commented<T> {
    pub made: T,
    pub workspace: Option<Workspace>,
}

impl Store {
    /// Returns the comment as stored, with its revision. The comment being written on its line goes with it, if it says
    /// what's saved and adds a comment rather than changing one, since this is it saved. One that says anything else
    /// has been typed in since, so it stays.
    pub fn add_comment(
        &self,
        comment: &DiffComment,
        client_request_id: Option<&ClientRequestId>,
    ) -> Result<Commented<Made<DiffComment>>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        if let Some(made) = made_by(&tx, client_request_id)? {
            return Ok(Commented { made: Made::Earlier(made), workspace: None });
        }
        tx.execute(
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
        let made = inserted(&tx, comment.id.as_str())?;
        let saves = |writing: &CommentDraft| {
            writing.editing.is_none()
                && (&writing.path, writing.side, writing.line, &writing.text)
                    == (&comment.path, comment.side, comment.line, &comment.text)
        };
        let workspace = change_writing(&tx, &comment.workspace_id, |writing| (!saves(&writing)).then_some(writing))?;
        tx.commit()?;
        Ok(Commented { made, workspace })
    }

    pub fn list_comments(&self, workspace_id: &WorkspaceId) -> Result<Vec<DiffComment>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {COMMENT_COLUMNS}, rev FROM diff_comments WHERE workspace_id = ? ORDER BY created_at_ms, id"
        ))?;
        let rows = statement.query_map([workspace_id.as_str()], comment_from_row)?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    /// Replaces a comment's text, and returns it as stored, with its new revision. The comment being written goes with
    /// it, if it changes this comment and says what's saved, as adding one does.
    pub fn edit_comment(&self, id: &CommentId, text: &str) -> Result<Commented<DiffComment>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let changed = tx.execute("UPDATE diff_comments SET text = ? WHERE id = ?", params![text, id.as_str()])?;
        if changed == 0 {
            return Err(StoreError::NotFound { kind: "comment", id: id.to_string() });
        }
        let query = format!("SELECT {COMMENT_COLUMNS}, rev FROM diff_comments WHERE id = ?");
        let edited = tx.query_row(&query, [id.as_str()], comment_from_row)?;
        let saves = |writing: &CommentDraft| writing.editing.as_ref() == Some(id) && writing.text == text;
        let workspace = change_writing(&tx, &edited.workspace_id, |writing| (!saves(&writing)).then_some(writing))?;
        tx.commit()?;
        Ok(Commented { made: edited, workspace })
    }

    /// Returns the revision of the removal, or `None` if there was nothing to remove. A comment being changed as it's
    /// deleted, such as from another client, becomes a new one, so what's typed in it isn't lost.
    pub fn delete_comment(&self, id: &CommentId) -> Result<Commented<Option<Rev>>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let on: Option<WorkspaceId> = tx
            .query_row("SELECT workspace_id FROM diff_comments WHERE id = ?", [id.as_str()], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .map(Into::into);
        let Some(on) = on else { return Ok(Commented { made: None, workspace: None }) };
        let workspace = change_writing(&tx, &on, |writing| {
            let editing_this = writing.editing.as_ref() == Some(id);
            Some(if editing_this { CommentDraft { editing: None, ..writing } } else { writing })
        })?;
        tx.execute("DELETE FROM diff_comments WHERE id = ?", [id.as_str()])?;
        let rev = current_rev(&tx)?;
        tx.commit()?;
        Ok(Commented { made: Some(rev), workspace })
    }
}

/// Replaces the comment being written on the workspace's diff with what `change` makes of it, inside a change to its
/// comments. Returns the workspace if that changed it, and `None` if nothing was being written or `change` left it as
/// it was.
fn change_writing(
    conn: &Connection,
    id: &WorkspaceId,
    change: impl FnOnce(CommentDraft) -> Option<CommentDraft>,
) -> Result<Option<Workspace>, StoreError> {
    let Some(writing) = workspace_on(conn, id)?.comment_draft else { return Ok(None) };
    let changed = change(writing.clone());
    if changed.as_ref() == Some(&writing) {
        return Ok(None);
    }
    let json = changed.as_ref().map(serde_json::to_string).transpose()?;
    conn.execute("UPDATE workspaces SET comment_draft = ? WHERE id = ?", params![json, id.as_str()])?;
    Ok(Some(workspace_on(conn, id)?))
}
