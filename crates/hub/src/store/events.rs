//! Each session's event log.

use protocol::event::{AgentTask, ApprovalOutcome, EventBody, SessionEvent};
use protocol::model::{
    ApprovalId, ClientMessageId, Place, PromptLine, RunId, Seq, SessionId, SessionStatus, ToolUseId, WorkspaceId,
};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::prompts::note_prompts;
use super::queue::take_out_recorded;
use super::subagents::note_subagents;
use super::{Appended, Store, StoreError, lock, now_ms, text_of};

/// What an event changes about where a session's agent has got to, which commits with the event that brings it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusChange {
    pub status: SessionStatus,
    pub turn: TurnEdge,
    /// What it's running in the background now, if the event changes that.
    pub background_tasks: Option<Vec<AgentTask>>,
}

/// What a change of status does to the session's turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnEdge {
    /// The event starts a turn, so its time becomes when the newest turn started.
    Starts,
    /// The event ends a turn, so it becomes the session's newest reply.
    Ends,
    /// It does neither, such as the agent stopping to ask, or going away mid-turn.
    Neither,
}

impl From<SessionStatus> for StatusChange {
    fn from(status: SessionStatus) -> Self {
        Self { status, turn: TurnEdge::Neither, background_tasks: None }
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

/// Makes `change` to the session, as part of the transaction that records the event at `seq`, at `ts_ms`.
fn change_session(
    tx: &Connection,
    session_id: &SessionId,
    seq: i64,
    ts_ms: i64,
    change: SessionChange,
) -> Result<(), StoreError> {
    let id = session_id.as_str();
    if let Some(held) = change.queue_held {
        tx.execute("UPDATE sessions SET queue_held = ? WHERE id = ?", params![held, id])?;
    }
    let Some(StatusChange { status, turn, background_tasks }) = change.status else { return Ok(()) };
    if let Some(tasks) = background_tasks {
        tx.execute(
            "UPDATE sessions SET background_tasks = ? WHERE id = ?",
            params![serde_json::to_string(&tasks)?, id],
        )?;
    }
    let status = text_of(&status);
    match turn {
        TurnEdge::Starts => tx.execute(
            "UPDATE sessions SET status = ?, turn_started_at_ms = ? WHERE id = ?",
            params![status, ts_ms, id],
        )?,
        TurnEdge::Ends => {
            tx.execute("UPDATE sessions SET status = ?, reply_seq = ? WHERE id = ?", params![status, seq, id])?
        }
        TurnEdge::Neither => tx.execute("UPDATE sessions SET status = ? WHERE id = ?", params![status, id])?,
    };
    Ok(())
}

/// Which events a read by place takes: the agent's own, or those of the subagent `subagent` started, from `from` on,
/// and none recorded after `through`, if it's set.
#[derive(Clone, Copy, Debug)]
pub struct Places<'a> {
    pub subagent: Option<&'a ToolUseId>,
    pub from: Place,
    pub through: Option<Seq>,
}

/// The event that closes an unanswered approval when nobody can answer it any more.
pub fn cancelled_approval(approval_id: ApprovalId) -> EventBody {
    EventBody::ApprovalResolved { approval_id, outcome: ApprovalOutcome::Cancelled }
}

impl Store {
    /// `change` commits with the event, so nobody can see one without the other. A client that reads status after an
    /// event gets at least that event's status, and the reply it made. A prompt recorded under the ID of one in the
    /// queue comes out of the queue in the same commit, so no prompt is ever both sent and waiting to be. The event
    /// takes the next place among those from its subagent, or the agent's own, in the same commit, so there's no gap.
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
            return Ok(Appended { event, duplicate: true, dequeued: false, untaken: None, subagents: Vec::new() });
        }
        let last_seq: Option<i64> = tx
            .query_row("SELECT last_seq FROM sessions WHERE id = ?", [session_id.as_str()], |row| row.get(0))
            .optional()?;
        let last_seq = last_seq.ok_or_else(|| StoreError::NotFound { kind: "session", id: session_id.to_string() })?;
        let seq = last_seq + 1;
        let ts_ms = now_ms();
        let encoded = serde_json::to_string(body)?;
        let subagent = body.subagent();
        let place: i64 = tx.query_row(
            "SELECT ifnull(max(place), 0) + 1 FROM events WHERE session_id = ? AND subagent IS ?",
            params![session_id.as_str(), subagent.map(ToolUseId::as_str)],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO events (session_id, seq, ts_ms, kind, client_msg_id, run, subagent, place, body, raw)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                session_id.as_str(),
                seq,
                ts_ms,
                body.kind(),
                client_msg_id.map(ClientMessageId::as_str),
                run.map(|run| run.get().cast_signed()),
                subagent.map(ToolUseId::as_str),
                place,
                encoded,
                raw
            ],
        )?;
        tx.execute(
            "UPDATE sessions SET last_seq = ?, updated_at_ms = ? WHERE id = ?",
            params![seq, ts_ms, session_id.as_str()],
        )?;
        change_session(&tx, session_id, seq, ts_ms, change.unwrap_or_default())?;
        let dequeued = match (body, client_msg_id) {
            (EventBody::UserPrompt { .. }, Some(client_msg_id)) => take_out_recorded(&tx, session_id, client_msg_id)?,
            _ => false,
        };
        let untaken = note_prompts(&tx, session_id, seq.cast_unsigned(), body)?;
        let event = SessionEvent {
            session_id: session_id.clone(),
            seq: seq.cast_unsigned(),
            ts_ms,
            run,
            subagent: subagent.cloned(),
            place: Place::new(place.cast_unsigned()),
            body: body.clone(),
            raw: None,
        };
        let subagents = note_subagents(&tx, &event)?;
        tx.commit()?;
        Ok(Appended { event, duplicate: false, dequeued, untaken, subagents })
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
            "SELECT {EVENT_COLUMNS}, {raw_column} FROM events WHERE session_id = ? AND seq >= ? ORDER BY seq LIMIT ?"
        ))?;
        let rows = statement.query(params![session_id.as_str(), from_seq.cast_signed(), limit])?;
        events_in(session_id, rows, max_bytes)
    }

    /// The events `places` names, with the place of the newest from the same agent or subagent, recorded after
    /// `through` or not. Always returns at least one event if any exist there, however large, as
    /// [`Store::read_events`] does.
    pub fn read_places(
        &self,
        session_id: &SessionId,
        places: Places<'_>,
        limit: u32,
        max_bytes: u64,
        include_raw: bool,
    ) -> Result<(Vec<SessionEvent>, Place), StoreError> {
        let Places { subagent, from, through } = places;
        let conn = lock(&self.reader);
        let raw_column = if include_raw { "raw" } else { "NULL" };
        let subagent = subagent.map(ToolUseId::as_str);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {EVENT_COLUMNS}, {raw_column} FROM events
             WHERE session_id = ? AND subagent IS ? AND place >= ? AND seq <= ? ORDER BY place LIMIT ?"
        ))?;
        let through = through.map_or(i64::MAX, Seq::cast_signed);
        let rows = statement.query(params![session_id.as_str(), subagent, from.get().cast_signed(), through, limit])?;
        let events = events_in(session_id, rows, max_bytes)?;
        let last: i64 = conn.query_row(
            "SELECT ifnull(max(place), 0) FROM events WHERE session_id = ? AND subagent IS ?",
            params![session_id.as_str(), subagent],
            |row| row.get(0),
        )?;
        Ok((events, Place::new(last.cast_unsigned())))
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
                | EventBody::ToolCallDropped { .. }
                | EventBody::AgentStarted { .. }
                | EventBody::AgentInit { .. }
                | EventBody::AgentState { .. }
                | EventBody::AgentTasks { .. }
                | EventBody::AgentLimits { .. }
                | EventBody::Assistant { .. }
                | EventBody::AgentUser { .. }
                | EventBody::TurnEnded { .. }
                | EventBody::AgentExited { .. }
                | EventBody::Notice { .. }
                | EventBody::AgentMode { .. }
                | EventBody::SubagentStarted { .. }
                | EventBody::SubagentEnded { .. }
                | EventBody::AgentOther { .. }
                | EventBody::Unknown => {}
            }
        }
        Ok(open)
    }

    /// Every prompt recorded in the session, oldest first, each as one line. It reads only prompts, by their kind, and
    /// of each only its text and how many files it carries, so a long session costs no more than its prompts' text.
    pub fn prompt_lines(&self, session_id: &SessionId) -> Result<Vec<PromptLine>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(
            "SELECT seq, json_extract(body, '$.text'), json_array_length(body, '$.attachments') FROM events \
             WHERE session_id = ? AND kind = 'user_prompt' ORDER BY seq",
        )?;
        lines_of(statement.query([session_id.as_str()])?)
    }

    /// Every prompt recorded in the workspace's chats, oldest first whichever chat it was in, each as one line, read as
    /// [`Store::prompt_lines`] reads one chat's. A line's `seq` is its place in its own chat's log.
    pub fn workspace_prompt_lines(&self, workspace_id: &WorkspaceId) -> Result<Vec<PromptLine>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(
            "SELECT e.seq, json_extract(e.body, '$.text'), json_array_length(e.body, '$.attachments') \
             FROM sessions s JOIN events e ON e.session_id = s.id AND e.kind = 'user_prompt' \
             WHERE s.workspace_id = ? ORDER BY e.ts_ms, s.created_at_ms, e.seq",
        )?;
        lines_of(statement.query([workspace_id.as_str()])?)
    }
}

/// The line for each row of a prompt's `seq`, text and how many files it carries.
fn lines_of(mut rows: rusqlite::Rows<'_>) -> Result<Vec<PromptLine>, StoreError> {
    let mut lines = Vec::new();
    while let Some(row) = rows.next()? {
        let seq = row.get::<_, i64>(0)?.cast_unsigned();
        // Borrowed rather than copied, since a pasted log can be long and only its start is kept. A JSON string
        // comes out as text, and a missing one as null.
        let text = match row.get_ref(1)? {
            ValueRef::Text(text) => String::from_utf8_lossy(text),
            ValueRef::Null | ValueRef::Integer(_) | ValueRef::Real(_) | ValueRef::Blob(_) => "".into(),
        };
        let files = u32::try_from(row.get::<_, Option<i64>>(2)?.unwrap_or(0)).unwrap_or(u32::MAX);
        lines.push(PromptLine::new(seq, &text, files));
    }
    Ok(lines)
}

/// The event already recorded with this `client_msg_id`, if there is one.
fn recorded_with(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
    client_msg_id: &ClientMessageId,
) -> Result<Option<SessionEvent>, StoreError> {
    let mut statement = tx.prepare_cached(&format!(
        "SELECT {EVENT_COLUMNS}, NULL FROM events WHERE session_id = ? AND client_msg_id = ?"
    ))?;
    let mut rows = statement.query(params![session_id.as_str(), client_msg_id.as_str()])?;
    let Some(row) = rows.next()? else { return Ok(None) };
    Ok(Some(event_in(session_id, row)?.0))
}

/// The events in `rows`, which each have [`EVENT_COLUMNS`] then the raw line, until they take `max_bytes`, and at
/// least one if there are any.
fn events_in(
    session_id: &SessionId,
    mut rows: rusqlite::Rows<'_>,
    max_bytes: u64,
) -> Result<Vec<SessionEvent>, StoreError> {
    let mut events = Vec::new();
    let mut bytes = 0u64;
    while let Some(row) = rows.next()? {
        let (event, size) = event_in(session_id, row)?;
        events.push(event);
        bytes += size;
        if bytes >= max_bytes {
            break;
        }
    }
    Ok(events)
}

/// What [`event_in`] reads, before the raw line or NULL.
const EVENT_COLUMNS: &str = "seq, ts_ms, run, subagent, place, body";

/// The event in a row of [`EVENT_COLUMNS`] and then its raw line, and how many bytes its body and raw line take.
fn event_in(session_id: &SessionId, row: &rusqlite::Row<'_>) -> Result<(SessionEvent, u64), StoreError> {
    let body: String = row.get(5)?;
    let raw: Option<String> = row.get(6)?;
    let size = (body.len() + raw.as_ref().map_or(0, String::len)) as u64;
    let event = SessionEvent {
        session_id: session_id.clone(),
        seq: row.get::<_, i64>(0)?.cast_unsigned(),
        ts_ms: row.get(1)?,
        run: row.get::<_, Option<i64>>(2)?.map(|run| RunId::new(run.cast_unsigned())),
        subagent: row.get::<_, Option<String>>(3)?.map(ToolUseId::from),
        place: Place::new(row.get::<_, i64>(4)?.cast_unsigned()),
        body: serde_json::from_str(&body)?,
        raw,
    };
    Ok((event, size))
}

#[cfg(test)]
mod tests;
