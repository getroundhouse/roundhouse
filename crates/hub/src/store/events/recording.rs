use super::super::approvals::note_approvals;
use super::super::claims::clear_claims;
use super::super::prompts::note_prompts;
use super::super::sent::{Sent, note_sent};
use super::super::subagents::note_subagents;
use super::super::{Appended, Store, StoreError, lock, now_ms, place_column, place_of, seq_column, seq_of};
use super::change::{SessionChange, change_session};
use protocol::event::{ApprovalOutcome, EventBody, SessionEvent};
use protocol::model::{ApprovalId, ClientMessageId, Place, RunId, Seq, SessionId, ToolUseId};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

impl Appended {
    /// The event recorded before under the same `client_msg_id`, which recording it again leaves as it was.
    pub(super) fn already_recorded(event: SessionEvent) -> Self {
        let (dequeued, by_the_person, account, untaken, approvals) = (false, false, None, None, None);
        let subagents = Vec::new();
        Appended {
            event,
            duplicate: true,
            dequeued,
            by_the_person,
            account,
            untaken,
            approvals,
            subagents,
            claims_taken_off: None,
        }
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
    pub(super) run: Option<RunId>,
    pub(super) body: &'a EventBody,
    pub(super) raw: Option<&'a str>,
    pub(super) client_msg_id: Option<&'a ClientMessageId>,
    pub(super) change: Option<SessionChange>,
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
    let Sent { dequeued, by_the_person, account } = note_sent(tx, session_id, ts_ms, run, body, client_msg_id)?;
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
    let claims_taken_off = clear_claims(tx, session_id, run, body)?;
    Ok(Appended {
        event,
        duplicate: false,
        dequeued,
        by_the_person,
        account,
        untaken,
        approvals,
        subagents,
        claims_taken_off,
    })
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
        let conn = lock(&self.writer);
        let value: i64 =
            conn.query_row("UPDATE counters SET value = value + 1 WHERE name = 'run' RETURNING value", [], |row| {
                row.get(0)
            })?;
        Ok(RunId::new(value.cast_unsigned()))
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
