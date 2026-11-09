//! Each session's event log.

use protocol::event::{AgentTask, ApprovalOutcome, EventBody, SessionEvent};
use protocol::model::{ApprovalId, ClientMessageId, RunId, Seq, SessionId, SessionStatus};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::{Appended, Store, StoreError, lock, now_ms, text_of};

/// What an event changes about where a session's agent has got to, which commits with the event that brings it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusChange {
    pub status: SessionStatus,
    /// The event ends a turn, so it becomes the session's newest reply.
    pub ends_turn: bool,
    /// What it's running in the background now, if the event changes that.
    pub background_tasks: Option<Vec<AgentTask>>,
}

impl From<SessionStatus> for StatusChange {
    fn from(status: SessionStatus) -> Self {
        Self { status, ends_turn: false, background_tasks: None }
    }
}

/// What an event changes about its session, which commits with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionChange {
    pub status: Option<StatusChange>,
    /// Whether the queue is held from here on.
    pub queue_held: Option<bool>,
}

impl SessionChange {
    pub fn is_none(&self) -> bool {
        self.status.is_none() && self.queue_held.is_none()
    }
}

impl From<StatusChange> for SessionChange {
    fn from(status: StatusChange) -> Self {
        Self { status: Some(status), queue_held: None }
    }
}

impl From<SessionStatus> for SessionChange {
    fn from(status: SessionStatus) -> Self {
        StatusChange::from(status).into()
    }
}

/// Makes `change` to the session, as part of the transaction that records the event at `seq`.
fn change_session(tx: &Connection, session_id: &SessionId, seq: i64, change: SessionChange) -> Result<(), StoreError> {
    if let Some(held) = change.queue_held {
        tx.execute("UPDATE sessions SET queue_held = ? WHERE id = ?", params![held, session_id.as_str()])?;
    }
    let Some(StatusChange { status, ends_turn, background_tasks }) = change.status else { return Ok(()) };
    if let Some(tasks) = background_tasks {
        tx.execute(
            "UPDATE sessions SET background_tasks = ? WHERE id = ?",
            params![serde_json::to_string(&tasks)?, session_id.as_str()],
        )?;
    }
    let status = text_of(&status);
    if ends_turn {
        tx.execute(
            "UPDATE sessions SET status = ?, reply_seq = ? WHERE id = ?",
            params![status, seq, session_id.as_str()],
        )?;
    } else {
        tx.execute("UPDATE sessions SET status = ? WHERE id = ?", params![status, session_id.as_str()])?;
    }
    Ok(())
}

/// The event that closes an unanswered approval when nobody can answer it any more.
pub fn cancelled_approval(approval_id: ApprovalId) -> EventBody {
    EventBody::ApprovalResolved { approval_id, outcome: ApprovalOutcome::Cancelled }
}

impl Store {
    /// `change` commits with the event, so nobody can see one without the other. A client that reads status after an
    /// event gets at least that event's status, and the reply it made.
    pub fn append(
        &self,
        session_id: &SessionId,
        run: Option<RunId>,
        body: &EventBody,
        raw: Option<&str>,
        client_msg_id: Option<&ClientMessageId>,
        change: Option<SessionChange>,
    ) -> Result<Appended, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(client_msg_id) = client_msg_id
            && let Some(event) = recorded_with(&tx, session_id, client_msg_id)?
        {
            return Ok(Appended { event, duplicate: true });
        }
        let last_seq: Option<i64> = tx
            .query_row("SELECT last_seq FROM sessions WHERE id = ?", [session_id.as_str()], |row| row.get(0))
            .optional()?;
        let last_seq = last_seq.ok_or_else(|| StoreError::NotFound { kind: "session", id: session_id.to_string() })?;
        let seq = last_seq + 1;
        let ts_ms = now_ms();
        let encoded = serde_json::to_string(body)?;
        tx.execute(
            "INSERT INTO events (session_id, seq, ts_ms, kind, client_msg_id, run, body, raw)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                session_id.as_str(),
                seq,
                ts_ms,
                body.kind(),
                client_msg_id.map(ClientMessageId::as_str),
                run.map(|run| run.get().cast_signed()),
                encoded,
                raw
            ],
        )?;
        tx.execute(
            "UPDATE sessions SET last_seq = ?, updated_at_ms = ? WHERE id = ?",
            params![seq, ts_ms, session_id.as_str()],
        )?;
        change_session(&tx, session_id, seq, change.unwrap_or_default())?;
        tx.commit()?;
        let event = SessionEvent {
            session_id: session_id.clone(),
            seq: seq.cast_unsigned(),
            ts_ms,
            run,
            body: body.clone(),
            raw: None,
        };
        Ok(Appended { event, duplicate: false })
    }

    /// Always returns at least one event if any exist at or after `from_seq`, however large.
    pub fn read_events(
        &self,
        session_id: &SessionId,
        from_seq: Seq,
        limit: u32,
        max_bytes: u64,
        include_raw: bool,
    ) -> Result<Vec<SessionEvent>, StoreError> {
        let conn = lock(&self.reader);
        let raw_column = if include_raw { "raw" } else { "NULL" };
        let mut statement = conn.prepare_cached(&format!(
            "SELECT seq, ts_ms, run, body, {raw_column} FROM events WHERE session_id = ? AND seq >= ? ORDER BY seq LIMIT ?"
        ))?;
        let mut rows = statement.query(params![session_id.as_str(), from_seq.cast_signed(), limit])?;
        let mut events = Vec::new();
        let mut bytes = 0u64;
        while let Some(row) = rows.next()? {
            let body: String = row.get(3)?;
            let raw: Option<String> = row.get(4)?;
            bytes += (body.len() + raw.as_ref().map_or(0, String::len)) as u64;
            events.push(SessionEvent {
                session_id: session_id.clone(),
                seq: row.get::<_, i64>(0)?.cast_unsigned(),
                ts_ms: row.get(1)?,
                run: run_of(row, 2)?,
                body: serde_json::from_str(&body)?,
                raw,
            });
            if bytes >= max_bytes {
                break;
            }
        }
        Ok(events)
    }

    /// The next run's ID. It's taken before the launch and never handed out again, so a run is
    /// unique even if it never records anything.
    pub fn next_run(&self) -> Result<RunId, StoreError> {
        let conn = lock(&self.writer);
        let value: i64 =
            conn.query_row("UPDATE counters SET value = value + 1 WHERE name = 'run' RETURNING value", [], |row| {
                row.get(0)
            })?;
        Ok(RunId::new(value.cast_unsigned()))
    }

    /// Approvals requested and never resolved.
    pub fn open_approvals(&self, session_id: &SessionId) -> Result<Vec<ApprovalId>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn
            .prepare_cached(
                "SELECT body FROM events WHERE session_id = ? AND kind IN ('approval_requested', 'approval_resolved') ORDER BY seq",
            )
            ?;
        let rows = statement.query_map([session_id.as_str()], |row| row.get::<_, String>(0))?;
        let mut open = Vec::new();
        for body in rows {
            let body: EventBody = serde_json::from_str(&body?)?;
            match body {
                EventBody::ApprovalRequested { approval_id, .. } => open.push(approval_id),
                EventBody::ApprovalResolved { approval_id, .. } => open.retain(|id| *id != approval_id),
                EventBody::UserPrompt { .. }
                | EventBody::PromptTaken { .. }
                | EventBody::PromptDropped { .. }
                | EventBody::AgentStarted { .. }
                | EventBody::AgentInit { .. }
                | EventBody::AgentState { .. }
                | EventBody::AgentTasks { .. }
                | EventBody::Assistant { .. }
                | EventBody::AgentUser { .. }
                | EventBody::TurnEnded { .. }
                | EventBody::AgentExited { .. }
                | EventBody::Notice { .. }
                | EventBody::AgentMode { .. }
                | EventBody::AgentOther { .. }
                | EventBody::Unknown => {}
            }
        }
        Ok(open)
    }
}

/// The event already recorded with this `client_msg_id`, if there is one.
fn recorded_with(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
    client_msg_id: &ClientMessageId,
) -> Result<Option<SessionEvent>, StoreError> {
    let existing = tx
        .query_row(
            "SELECT seq, ts_ms, run, body FROM events WHERE session_id = ? AND client_msg_id = ?",
            params![session_id.as_str(), client_msg_id.as_str()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, run_of(row, 2)?, row.get::<_, String>(3)?)),
        )
        .optional()?;
    let Some((seq, ts_ms, run, body)) = existing else {
        return Ok(None);
    };
    let body = serde_json::from_str(&body)?;
    Ok(Some(SessionEvent { session_id: session_id.clone(), seq: seq.cast_unsigned(), ts_ms, run, body, raw: None }))
}

fn run_of(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<Option<RunId>> {
    Ok(row.get::<_, Option<i64>>(column)?.map(|run| RunId::new(run.cast_unsigned())))
}
