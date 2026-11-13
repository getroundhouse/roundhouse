//! Each workspace's other worktrees, made for its work. A record stays once its worktree is removed, with its branch,
//! so a worktree can be made on the branch again, and no two of a workspace's that aren't removed share a path.

use protocol::model::{AskKey, Branches, ChildStatus, ChildWorktree, ChildWorktreeId, MadeBy, SessionId, WorkspaceId};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::made::Made;
use super::{Store, StoreError, json_at, lock, now_ms, rev_of};

#[cfg(test)]
mod tests;

const CHILD_COLUMNS: &str =
    "id, workspace_id, path, branch, session_id, made_by, created_at_ms, state, removed_at_ms, branches, rev";

/// How `state` spells a record that isn't removed.
const THERE: &str = "state != 'removed'";

/// The record a session's ask made, by its key, which is unique within the session.
const BY_ASK: &str = "WHERE session_id = ? AND ask_key = ?";

fn child_from_row(row: &Row<'_>) -> rusqlite::Result<ChildWorktree> {
    let status = match (row.get::<_, String>(7)?.as_str(), row.get::<_, Option<i64>>(8)?) {
        ("creating", _) => ChildStatus::Creating,
        ("ready", _) => ChildStatus::Ready,
        ("removed", Some(removed_at_ms)) => ChildStatus::Removed { removed_at_ms },
        _ => ChildStatus::Unknown,
    };
    Ok(ChildWorktree {
        id: row.get::<_, String>(0)?.into(),
        workspace_id: row.get::<_, String>(1)?.into(),
        path: row.get(2)?,
        branch: row.get::<_, String>(3)?.as_str().into(),
        session_id: row.get::<_, String>(4)?.into(),
        made_by: json_at(row, 5)?.unwrap_or(MadeBy::Unknown),
        created_at_ms: row.get(6)?,
        status,
        branches: json_at(row, 9)?,
        rev: rev_of(row, 10)?,
    })
}

/// The record as `conn` reads it, such as the writer inside a change, so its revision is that change's.
fn child_on(conn: &Connection, id: &ChildWorktreeId) -> Result<ChildWorktree, StoreError> {
    conn.query_row(&format!("SELECT {CHILD_COLUMNS} FROM child_worktrees WHERE id = ?"), [id.as_str()], child_from_row)
        .optional()?
        .ok_or_else(|| StoreError::NotFound { kind: "worktree", id: id.to_string() })
}

impl Store {
    /// Records `child` as being made, before its worker is asked for it, unless its session's ask with `key` made one
    /// already, which it returns instead. `None` if another of the workspace's worktrees that isn't removed has its
    /// path.
    pub fn insert_child(&self, child: &ChildWorktree, key: &AskKey) -> Result<Option<Made<ChildWorktree>>, StoreError> {
        let conn = lock(&self.writer);
        let select = format!("SELECT {CHILD_COLUMNS} FROM child_worktrees {BY_ASK}");
        let asked = params![child.session_id.as_str(), key.as_str()];
        if let Some(earlier) = conn.query_row(&select, asked, child_from_row).optional()? {
            return Ok(Some(Made::Earlier(earlier)));
        }
        let added = conn.execute(
            "INSERT INTO child_worktrees
                 (id, workspace_id, path, branch, session_id, made_by, created_at_ms, state, ask_key)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 'creating', ?)
                 ON CONFLICT DO NOTHING",
            params![
                child.id.as_str(),
                child.workspace_id.as_str(),
                child.path,
                child.branch.as_str(),
                child.session_id.as_str(),
                serde_json::to_string(&child.made_by)?,
                child.created_at_ms,
                key.as_str(),
            ],
        )?;
        if added == 0 {
            return Ok(None);
        }
        child_on(&conn, &child.id).map(|made| Some(Made::New(made)))
    }

    /// The record the session's ask with `key` made, if it made one.
    pub fn child_for_ask(&self, session_id: &SessionId, key: &AskKey) -> Result<Option<ChildWorktree>, StoreError> {
        let select = format!("SELECT {CHILD_COLUMNS} FROM child_worktrees {BY_ASK}");
        let asked = params![session_id.as_str(), key.as_str()];
        Ok(lock(&self.reader).query_row(&select, asked, child_from_row).optional()?)
    }

    /// Records one being made as there, with its branches then. `None` if it isn't being made, as when it was removed
    /// meanwhile, which then stays removed.
    pub fn child_ready(
        &self,
        id: &ChildWorktreeId,
        branches: Option<&Branches>,
    ) -> Result<Option<ChildWorktree>, StoreError> {
        let conn = lock(&self.writer);
        let branches = branches.map(serde_json::to_string).transpose()?;
        let made = conn.execute(
            "UPDATE child_worktrees SET state = 'ready', branches = ? WHERE id = ? AND state = 'creating'",
            params![branches, id.as_str()],
        )?;
        (made > 0).then(|| child_on(&conn, id)).transpose()
    }

    /// Forgets one being made that its worker didn't make, so its path is free for the next name.
    pub fn discard_child(&self, id: &ChildWorktreeId) -> Result<(), StoreError> {
        lock(&self.writer).execute("DELETE FROM child_worktrees WHERE id = ? AND state = 'creating'", [id.as_str()])?;
        Ok(())
    }

    /// Records it removed, keeping the record, with its branch.
    pub fn child_removed(&self, id: &ChildWorktreeId) -> Result<ChildWorktree, StoreError> {
        let conn = lock(&self.writer);
        conn.execute(
            "UPDATE child_worktrees SET state = 'removed', removed_at_ms = ? WHERE id = ?",
            params![now_ms(), id.as_str()],
        )?;
        child_on(&conn, id)
    }

    /// The workspace's other worktrees that are there or being made, oldest first.
    pub fn children_of(&self, workspace_id: &WorkspaceId) -> Result<Vec<ChildWorktree>, StoreError> {
        self.children_where(workspace_id, THERE)
    }

    /// Every one the workspace has had, those removed too, oldest first.
    pub fn every_child_of(&self, workspace_id: &WorkspaceId) -> Result<Vec<ChildWorktree>, StoreError> {
        self.children_where(workspace_id, "1")
    }

    fn children_where(&self, workspace_id: &WorkspaceId, which: &str) -> Result<Vec<ChildWorktree>, StoreError> {
        let conn = lock(&self.reader);
        let select = format!(
            "SELECT {CHILD_COLUMNS} FROM child_worktrees
                 WHERE workspace_id = ? AND {which} ORDER BY created_at_ms, rowid"
        );
        let mut statement = conn.prepare_cached(&select)?;
        let rows = statement.query_map([workspace_id.as_str()], child_from_row)?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    /// The one of the workspace's other worktrees at `path` that's there or being made.
    pub fn child_at(&self, workspace_id: &WorkspaceId, path: &str) -> Result<Option<ChildWorktree>, StoreError> {
        let select =
            format!("SELECT {CHILD_COLUMNS} FROM child_worktrees WHERE workspace_id = ? AND path = ? AND {THERE}");
        let conn = lock(&self.reader);
        Ok(conn.query_row(&select, params![workspace_id.as_str(), path], child_from_row).optional()?)
    }

    /// Records what the worker read of the branches of the one at `path`. Returns it only when that changed it, so
    /// reading the same again moves no revision.
    pub fn set_child_branches(
        &self,
        workspace_id: &WorkspaceId,
        path: &str,
        branches: &Branches,
    ) -> Result<Option<ChildWorktree>, StoreError> {
        let conn = lock(&self.writer);
        let update = format!(
            "UPDATE child_worktrees SET branches = ?1
                 WHERE workspace_id = ?2 AND path = ?3 AND {THERE} AND branches IS NOT ?1
                 RETURNING id"
        );
        let text = serde_json::to_string(branches)?;
        let changed: Option<String> =
            conn.query_row(&update, params![text, workspace_id.as_str(), path], |row| row.get(0)).optional()?;
        changed.map(|id| child_on(&conn, &id.into())).transpose()
    }
}
