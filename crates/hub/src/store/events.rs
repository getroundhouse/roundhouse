//! Each session's event log.

use protocol::event::{AgentTask, ApprovalOutcome, EventBody, SessionEvent, TurnOutcome};
use protocol::model::{ApprovalId, ClientMessageId, Place, RunId, Seq, SessionId, SessionStatus, ToolUseId};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::approvals::note_approvals;
use super::prompts::note_prompts;
use super::sent::note_sent;
use super::subagents::note_subagents;
use super::{Appended, Store, StoreError, lock, now_ms, place_column, place_of, seq_column, seq_of, text_of};

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
    /// The event ends a turn, so it becomes the session's newest reply, which ended as it says, if it says.
    Ends(Option<TurnOutcome>),
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
    /// The run whose worker numbered what's recorded, and its number, which is the last recorded from that worker.
    pub recorded: Option<(RunId, u64)>,
}

impl SessionChange {
    /// Whether nothing on the session's record changes. Which number from its worker it has recorded isn't there.
    pub fn is_none(&self) -> bool {
        self.status.is_none() && self.queue_held.is_none()
    }
}

impl From<StatusChange> for SessionChange {
    fn from(status: StatusChange) -> Self {
        Self { status: Some(status), ..Self::default() }
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
    seq: Seq,
    ts_ms: i64,
    change: SessionChange,
) -> Result<(), StoreError> {
    let id = session_id.as_str();
    if let Some(held) = change.queue_held {
        tx.execute("UPDATE sessions SET queue_held = ? WHERE id = ?", params![held, id])?;
    }
    if let Some((run, through)) = change.recorded {
        tx.execute(
            "UPDATE sessions SET recorded_run = ?, recorded_through = ? WHERE id = ?",
            params![run.get().cast_signed(), through.cast_signed(), id],
        )?;
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
        TurnEdge::Ends(outcome) => tx.execute(
            "UPDATE sessions SET status = ?, reply_seq = ?, reply_outcome = ? WHERE id = ?",
            params![status, seq_column(seq), outcome.as_ref().map(text_of), id],
        )?,
        TurnEdge::Neither => tx.execute("UPDATE sessions SET status = ? WHERE id = ?", params![status, id])?,
    };
    Ok(())
}

impl Appended {
    /// The event recorded before under the same `client_msg_id`, which recording it again leaves as it was.
    fn already_recorded(event: SessionEvent) -> Self {
        let (dequeued, by_the_person, untaken, approvals) = (false, false, None, None);
        Appended { event, duplicate: true, dequeued, by_the_person, untaken, approvals, subagents: Vec::new() }
    }
}

/// Which events a read by place takes: the agent's own, or those of the subagent `subagent` started, from `from` on,
/// and none recorded after `through`, if it's set.
#[derive(Clone, Copy, Debug)]
pub struct Places<'a> {
    pub subagent: Option<&'a ToolUseId>,
    pub from: Place,
    pub through: Option<Seq>,
}

/// For a question answered or cancelled, lets go of what was being typed in answer to it, whoever answered it, since
/// nobody can send it now.
fn clear_answer(conn: &Connection, session_id: &SessionId, body: &EventBody) -> Result<(), StoreError> {
    let EventBody::ApprovalResolved { approval_id, .. } = body else { return Ok(()) };
    conn.execute(
        "UPDATE sessions SET answer_draft = NULL WHERE id = ?1 AND json_extract(answer_draft, '$.approval_id') = ?2",
        params![session_id.as_str(), approval_id.as_str()],
    )?;
    Ok(())
}

/// The event that closes an unanswered approval when nobody can answer it any more.
pub fn cancelled_approval(approval_id: ApprovalId) -> EventBody {
    EventBody::ApprovalResolved {
        approval_id,
        outcome: ApprovalOutcome::Cancelled,
        answers_by_text: None,
        answers_by_position: None,
        by_setting: false,
    }
}

/// An event to record, from the agent of `run` or from the hub, with what it changes on its session's record.
struct Recording<'a> {
    run: Option<RunId>,
    body: &'a EventBody,
    raw: Option<&'a str>,
    client_msg_id: Option<&'a ClientMessageId>,
    change: Option<SessionChange>,
}

/// Records `recording` as part of `tx`, as [`Store::append`] says.
fn append_in(tx: &Connection, session_id: &SessionId, recording: Recording<'_>) -> Result<Appended, StoreError> {
    let Recording { run, body, raw, client_msg_id, change } = recording;
    let last_seq = tx
        .query_row("SELECT last_seq FROM sessions WHERE id = ?", [session_id.as_str()], |row| seq_of(row, 0))
        .optional()?;
    let last_seq = last_seq.ok_or_else(|| StoreError::NotFound { kind: "session", id: session_id.to_string() })?;
    let seq = last_seq.next();
    let ts_ms = now_ms();
    let encoded = serde_json::to_string(body)?;
    let subagent = body.subagent();
    let place = last_place(tx, session_id, subagent)?.next();
    tx.execute(
        "INSERT INTO events (session_id, seq, ts_ms, kind, client_msg_id, run, subagent, place, body, raw)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            session_id.as_str(),
            seq_column(seq),
            ts_ms,
            body.kind(),
            client_msg_id.map(ClientMessageId::as_str),
            run.map(|run| run.get().cast_signed()),
            subagent.map(ToolUseId::as_str),
            place_column(place),
            encoded,
            raw
        ],
    )?;
    tx.execute(
        "UPDATE sessions SET last_seq = ?, updated_at_ms = ? WHERE id = ?",
        params![seq_column(seq), ts_ms, session_id.as_str()],
    )?;
    change_session(tx, session_id, seq, ts_ms, change.unwrap_or_default())?;
    let (dequeued, by_the_person) = note_sent(tx, session_id, ts_ms, run, body, client_msg_id)?;
    let untaken = note_prompts(tx, session_id, seq, body)?;
    let approvals = note_approvals(tx, session_id, seq, body)?;
    let event = SessionEvent {
        session_id: session_id.clone(),
        seq,
        ts_ms,
        run,
        subagent: subagent.cloned(),
        place,
        body: body.clone(),
        raw: None,
    };
    let subagents = note_subagents(tx, &event)?;
    clear_answer(tx, session_id, body)?;
    Ok(Appended { event, duplicate: false, dequeued, by_the_person, untaken, approvals, subagents })
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
            return Ok(Appended::already_recorded(event));
        }
        let appended = append_in(&tx, session_id, Recording { run, body, raw, client_msg_id, change })?;
        tx.commit()?;
        Ok(appended)
    }

    /// A request from the agent of `run`, with `change`, and the hub's `answer` to it, in one commit, so nobody can
    /// see the request waiting, as [`Store::append`] records each.
    pub fn append_answered(
        &self,
        session_id: &SessionId,
        run: Option<RunId>,
        (request, raw): (&EventBody, Option<&str>),
        answer: &EventBody,
        change: SessionChange,
    ) -> Result<[Appended; 2], StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let asked = Recording { run, body: request, raw, client_msg_id: None, change: Some(change) };
        let asked = append_in(&tx, session_id, asked)?;
        let answered = Recording { run: None, body: answer, raw: None, client_msg_id: None, change: None };
        let answered = append_in(&tx, session_id, answered)?;
        tx.commit()?;
        Ok([asked, answered])
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
        let rows = statement.query(params![session_id.as_str(), seq_column(from_seq), limit])?;
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
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {EVENT_COLUMNS}, {raw_column} FROM events
             WHERE session_id = ? AND subagent IS ? AND place >= ? AND seq <= ? ORDER BY place LIMIT ?"
        ))?;
        let (from, through) = (place_column(from), through.map_or(i64::MAX, seq_column));
        let rows =
            statement.query(params![session_id.as_str(), subagent.map(ToolUseId::as_str), from, through, limit])?;
        let events = events_in(session_id, rows, max_bytes)?;
        Ok((events, last_place(&conn, session_id, subagent)?))
    }

    /// The next run's ID. It's taken before the launch and never handed out again, so a run is
    /// unique even if it never records anything.
    pub fn next_run(&self) -> Result<RunId, StoreError> {
        self.bump("run").map(RunId::new)
    }
}

/// The place of the newest event from the agent itself, or from the subagent `subagent` started, or 0 if there's none.
fn last_place(conn: &Connection, session_id: &SessionId, subagent: Option<&ToolUseId>) -> rusqlite::Result<Place> {
    conn.query_row(
        "SELECT ifnull(max(place), 0) FROM events WHERE session_id = ? AND subagent IS ?",
        params![session_id.as_str(), subagent.map(ToolUseId::as_str)],
        |row| place_of(row, 0),
    )
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
        seq: seq_of(row, 0)?,
        ts_ms: row.get(1)?,
        run: row.get::<_, Option<i64>>(2)?.map(|run| RunId::new(run.cast_unsigned())),
        subagent: row.get::<_, Option<String>>(3)?.map(ToolUseId::from),
        place: place_of(row, 4)?,
        body: serde_json::from_str(&body)?,
        raw,
    };
    Ok((event, size))
}

#[cfg(test)]
pub(crate) mod tests;
