use super::super::{Store, StoreError, from_text, lock, text_of};
use protocol::model::{SessionId, ToolUseId, Wait, WaitEnd, WaitEnded, WaitId, WorkspaceId, WorkspaceStatus};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::HashSet;

const COLUMNS: &str = "id, session_id, workspace_id, number, call, until, started_at_ms, ended_at_ms, ended";

/// What GitHub had recorded of the pull request when a wait started, by GitHub's own clock, so a wait ends only on what
/// it records after.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    /// When it last left the merge queue without merging.
    pub left_queue_at_ms: Option<i64>,
    /// When a review last asked for changes.
    pub changes_requested_at_ms: Option<i64>,
}

impl Store {
    /// Starts a wait, in a chat that's open in a ready workspace, from what GitHub had recorded then, and ends the
    /// chat's open one as replaced, since an agent waits on one at a time. Returns the one it ended. One that ends as it
    /// starts never waits, so it replaces nothing, and the agent's open one goes on, as the agent hears.
    pub fn start_wait(&self, wait: &Wait, seen: Seen) -> Result<Option<WaitId>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let open: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM sessions JOIN workspaces ON workspaces.id = sessions.workspace_id
                 WHERE sessions.id = ? AND workspaces.id = ? AND sessions.closed_at_ms IS NULL AND workspaces.status = ?)",
            params![wait.session_id.as_str(), wait.workspace_id.as_str(), text_of(&WorkspaceStatus::Ready)],
            |row| row.get(0),
        )?;
        if !open {
            return Err(StoreError::NotFound {
                kind: "open chat in a ready workspace",
                id: wait.session_id.to_string(),
            });
        }
        let replaced = match wait.ended {
            None => call_off(&tx, &wait.session_id, None, wait.started_at_ms, WaitEnd::Replaced)?,
            Some(_) => None,
        };
        tx.prepare_cached(&format!(
            "INSERT INTO waits ({COLUMNS}, seen_left_queue_at_ms, seen_changes_requested_at_ms)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))?
        .execute(params![
            wait.id.as_str(),
            wait.session_id.as_str(),
            wait.workspace_id.as_str(),
            wait.number,
            wait.call.as_ref().map(ToolUseId::as_str),
            text_of(&wait.until),
            wait.started_at_ms,
            wait.ended.map(|ended| ended.at_ms),
            wait.ended.map(|ended| text_of(&ended.how)),
            seen.left_queue_at_ms,
            seen.changes_requested_at_ms,
        ])?;
        tx.commit()?;
        Ok(replaced)
    }

    /// Ends the workspace's open waits as cancelled, as it starts being archived. Returns those it ended.
    pub fn call_off_workspace_waits(&self, workspace_id: &WorkspaceId, at_ms: i64) -> Result<Vec<Wait>, StoreError> {
        let conn = lock(&self.writer);
        let mut query = conn.prepare_cached(&format!(
            "UPDATE waits SET ended_at_ms = ?, ended = ? WHERE workspace_id = ? AND ended_at_ms IS NULL RETURNING {COLUMNS}"
        ))?;
        let waits =
            query.query_map(params![at_ms, text_of(&WaitEnd::Cancelled), workspace_id.as_str()], wait_from_row)?;
        Ok(waits.collect::<rusqlite::Result<_>>()?)
    }

    /// Ends the session's wait `wait` as the person calling it off, if it's the one the session has open. Returns it,
    /// if it was.
    pub fn call_off_session_wait(
        &self,
        session_id: &SessionId,
        wait: &WaitId,
        at_ms: i64,
    ) -> Result<Option<WaitId>, StoreError> {
        Ok(call_off(&lock(&self.writer), session_id, Some(wait), at_ms, WaitEnd::CalledOffByPerson)?)
    }

    pub fn wait(&self, id: &WaitId) -> Result<Wait, StoreError> {
        let conn = lock(&self.reader);
        let mut query = conn.prepare_cached(&format!("SELECT {COLUMNS} FROM waits WHERE id = ?"))?;
        let wait = query.query_row([id.as_str()], wait_from_row).optional()?;
        wait.ok_or_else(|| StoreError::NotFound { kind: "wait", id: id.to_string() })
    }

    /// The session's newest wait that `call` started, whether or not it has ended.
    pub fn wait_of_call(&self, session_id: &SessionId, call: &ToolUseId) -> Result<Option<Wait>, StoreError> {
        let conn = lock(&self.reader);
        let mut query = conn.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM waits WHERE session_id = ? AND call = ? ORDER BY started_at_ms DESC, id DESC LIMIT 1"
        ))?;
        Ok(query.query_row([session_id.as_str(), call.as_str()], wait_from_row).optional()?)
    }

    /// Ends the wait, unless it has ended already, and returns it if this ended it.
    pub fn end_wait(&self, id: &WaitId, ended: WaitEnded) -> Result<Option<Wait>, StoreError> {
        let conn = lock(&self.writer);
        let mut update = conn.prepare_cached(&format!(
            "UPDATE waits SET ended_at_ms = ?, ended = ? WHERE id = ? AND ended_at_ms IS NULL RETURNING {COLUMNS}"
        ))?;
        Ok(update.query_row(params![ended.at_ms, text_of(&ended.how), id.as_str()], wait_from_row).optional()?)
    }

    /// After a read of the workspace's pull request: ends each of its open waits that `ends` says the read ends, from
    /// the wait and what was seen as it started, at `at_ms`, all at once. Returns those it ended.
    pub fn judge_waits(
        &self,
        id: &WorkspaceId,
        at_ms: i64,
        mut ends: impl FnMut(&Wait, Seen) -> Option<WaitEnd>,
    ) -> Result<Vec<Wait>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let open = tx
            .prepare_cached(&format!(
                "SELECT {COLUMNS}, seen_left_queue_at_ms, seen_changes_requested_at_ms FROM waits
                     WHERE workspace_id = ? AND ended_at_ms IS NULL ORDER BY started_at_ms, id"
            ))?
            .query_map([id.as_str()], |row| {
                let seen = Seen { left_queue_at_ms: row.get(9)?, changes_requested_at_ms: row.get(10)? };
                Ok((wait_from_row(row)?, seen))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut ended = Vec::new();
        for (mut wait, seen) in open {
            let Some(how) = ends(&wait, seen) else { continue };
            let how = WaitEnded { at_ms, how };
            tx.prepare_cached("UPDATE waits SET ended_at_ms = ?, ended = ? WHERE id = ?")?.execute(params![
                how.at_ms,
                text_of(&how.how),
                wait.id.as_str()
            ])?;
            wait.ended = Some(how);
            ended.push(wait);
        }
        tx.commit()?;
        Ok(ended)
    }

    /// Every session with a wait that hasn't ended.
    pub fn sessions_waiting(&self) -> Result<HashSet<SessionId>, StoreError> {
        let conn = lock(&self.reader);
        let mut query = conn.prepare_cached("SELECT DISTINCT session_id FROM waits WHERE ended_at_ms IS NULL")?;
        let ids = query.query_map([], |row| row.get::<_, String>(0))?;
        Ok(ids.map(|id| id.map(SessionId::from)).collect::<rusqlite::Result<_>>()?)
    }

    /// Every workspace with a wait that hasn't ended, whose pull request is read at the soonest.
    pub fn workspaces_waited_on(&self) -> Result<HashSet<WorkspaceId>, StoreError> {
        let conn = lock(&self.reader);
        let mut query = conn.prepare_cached("SELECT DISTINCT workspace_id FROM waits WHERE ended_at_ms IS NULL")?;
        let ids = query.query_map([], |row| row.get::<_, String>(0))?;
        Ok(ids.map(|id| id.map(WorkspaceId::from)).collect::<rusqlite::Result<_>>()?)
    }
}

/// Ends the chat's open wait, if it has one, and it's `wait` when that's given, as called off `how`.
fn call_off(
    conn: &Connection,
    session_id: &SessionId,
    wait: Option<&WaitId>,
    at_ms: i64,
    how: WaitEnd,
) -> rusqlite::Result<Option<WaitId>> {
    conn.prepare_cached(
        "UPDATE waits SET ended_at_ms = ?1, ended = ?2
             WHERE session_id = ?3 AND (?4 IS NULL OR id = ?4) AND ended_at_ms IS NULL RETURNING id",
    )?
    .query_row(params![at_ms, text_of(&how), session_id.as_str(), wait.map(WaitId::as_str)], |row| {
        row.get::<_, String>(0)
    })
    .optional()
    .map(|id| id.map(WaitId::from))
}

fn wait_from_row(row: &Row<'_>) -> rusqlite::Result<Wait> {
    let ended = match (row.get(7)?, row.get::<_, Option<String>>(8)?) {
        (Some(at_ms), Some(how)) => Some(WaitEnded { at_ms, how: from_text(how)? }),
        _ => None,
    };
    Ok(Wait {
        id: row.get::<_, String>(0)?.into(),
        session_id: row.get::<_, String>(1)?.into(),
        workspace_id: row.get::<_, String>(2)?.into(),
        number: row.get(3)?,
        call: row.get::<_, Option<String>>(4)?.map(Into::into),
        until: from_text(row.get(5)?)?,
        started_at_ms: row.get(6)?,
        ended,
    })
}
