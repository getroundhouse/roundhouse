use super::super::approvals::note_approvals;
use super::super::claims::clear_claims;
use super::super::prompts::note_prompts;
use super::super::sent::{Sent, note_sent};
use super::super::subagents::note_subagents;
use super::super::{
    Appended, Store, StoreError, from_text, lock, now_ms, place_column, place_of, seq_column, seq_of, text_of,
};
use super::change::{SessionChange, change_session};
use protocol::client::log::ReadLimit;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ApprovalId, ClientMessageId, InteractionMode, Place, RunId, Seq, SessionId, SubagentId};
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

/// Which events a read by place takes: the agent's own, or those of the subagent `subagent`, from `from` on,
/// and none recorded after `through`, if it's set.
#[derive(Clone, Copy, Debug)]
pub struct Places<'a> {
    pub subagent: Option<&'a SubagentId>,
    pub from: Place,
    pub through: Option<Seq>,
}

/// For a request decided or cancelled, lets go of what was being drafted on it, whoever decided it, since nobody can
/// send it now.
fn clear_decision_draft(conn: &Connection, session_id: &SessionId, body: &EventBody) -> Result<(), StoreError> {
    let EventBody::ApprovalResolved { approval_id, .. } = body else { return Ok(()) };
    conn.execute(
        "UPDATE sessions SET decision_draft = NULL WHERE id = ?1 AND json_extract(decision_draft, '$.approval_id') = ?2",
        params![session_id.as_str(), approval_id.as_str()],
    )?;
    Ok(())
}

/// The event that closes an unanswered approval when nobody can answer it any more.
pub fn cancelled_approval(approval_id: ApprovalId) -> EventBody {
    EventBody::ApprovalResolved { approval_id, decision: None, by_setting: false }
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
    // Before the event's row, so it's stamped with the mode it leaves the session in.
    change_session(tx, session_id, seq, ts_ms, change.unwrap_or_default())?;
    let interaction = interaction_of(tx, session_id)?;
    tx.execute(
        "INSERT INTO events (session_id, seq, ts_ms, kind, client_msg_id, run, subagent, place, interaction, body, raw)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            session_id.as_str(),
            seq_column(seq),
            ts_ms,
            body.kind(),
            client_msg_id.map(ClientMessageId::as_str),
            run.map(|run| run.get().cast_signed()),
            subagent.map(SubagentId::as_str),
            place_column(place),
            interaction_column(interaction),
            encoded,
            raw
        ],
    )?;
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
        interaction,
        body: body.clone(),
        raw: None,
    };
    let subagents = note_subagents(tx, &event)?;
    clear_decision_draft(tx, session_id, body)?;
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
        events_in(session_id, rows, limit, max_bytes)
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
        let mut conn = lock(&self.reader);
        // One transaction, so the newest place is as of the events read, and an event the agent records between the
        // two can't put it past them when nothing else does.
        let tx = conn.transaction()?;
        let raw_column = if include_raw { "raw" } else { "NULL" };
        let events = {
            let mut statement = tx.prepare_cached(&format!(
                "SELECT {EVENT_COLUMNS}, {raw_column} FROM events
                 WHERE session_id = ? AND subagent IS ? AND place >= ? AND seq <= ? ORDER BY place LIMIT ?"
            ))?;
            let (from, through) = (place_column(from), through.map_or(i64::MAX, seq_column));
            let rows = statement.query(params![
                session_id.as_str(),
                subagent.map(SubagentId::as_str),
                from,
                through,
                limit
            ])?;
            events_in(session_id, rows, limit, max_bytes)?
        };
        let last = last_place(&tx, session_id, subagent)?;
        tx.commit()?;
        Ok((events, last))
    }

    /// The agent's own newest failed request, a subagent's left out, if it has had one.
    pub fn newest_own_failure(&self, session_id: &SessionId) -> Result<Option<SessionEvent>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&newest_own_failure())?;
        let mut rows = statement.query(params![session_id.as_str()])?;
        let Some(row) = rows.next()? else { return Ok(None) };
        Ok(Some(event_in(session_id, row)?.0))
    }

    /// The next run's ID. It's taken before the launch and never handed out again, so a run is
    /// unique even if it never records anything.
    pub fn next_run(&self) -> Result<RunId, StoreError> {
        self.bump("run").map(RunId::new)
    }
}

/// The agent's own newest failed request, as [`EVENT_COLUMNS`] then a NULL raw line.
pub(super) fn newest_own_failure() -> String {
    format!(
        "SELECT {EVENT_COLUMNS}, NULL FROM events
         WHERE session_id = ? AND kind = 'request_failed' AND subagent IS NULL ORDER BY seq DESC LIMIT 1"
    )
}

/// The place of the newest event from the agent itself, or from the subagent `subagent`, or 0 if there's none.
fn last_place(conn: &Connection, session_id: &SessionId, subagent: Option<&SubagentId>) -> rusqlite::Result<Place> {
    conn.query_row(
        "SELECT ifnull(max(place), 0) FROM events WHERE session_id = ? AND subagent IS ?",
        params![session_id.as_str(), subagent.map(SubagentId::as_str)],
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

/// The events in `rows`, which each have [`EVENT_COLUMNS`] then the raw line, as far as a read of `limit` of them and
/// `max_bytes` goes, as [`ReadLimit::cut`] says.
fn events_in(
    session_id: &SessionId,
    rows: rusqlite::Rows<'_>,
    limit: u32,
    max_bytes: u64,
) -> Result<Vec<SessionEvent>, StoreError> {
    ReadLimit { events: limit, bytes: max_bytes }.cut(rows.and_then(|row| event_in(session_id, row)))
}

/// What [`event_in`] reads, before the raw line or NULL.
const EVENT_COLUMNS: &str = "seq, ts_ms, run, subagent, place, interaction, body";

/// The event in a row of [`EVENT_COLUMNS`] and then its raw line, and how many bytes its body and raw line take.
fn event_in(session_id: &SessionId, row: &rusqlite::Row<'_>) -> Result<(SessionEvent, u64), StoreError> {
    let body: String = row.get(6)?;
    let raw: Option<String> = row.get(7)?;
    let size = (body.len() + raw.as_ref().map_or(0, String::len)) as u64;
    let event = SessionEvent {
        session_id: session_id.clone(),
        seq: seq_of(row, 0)?,
        ts_ms: row.get(1)?,
        run: row.get::<_, Option<i64>>(2)?.map(|run| RunId::new(run.cast_unsigned())),
        subagent: row.get::<_, Option<String>>(3)?.map(SubagentId::from),
        place: place_of(row, 4)?,
        interaction: row.get::<_, Option<String>>(5)?.map_or(Ok(InteractionMode::Typing), from_text)?,
        body: serde_json::from_str(&body)?,
        raw,
    };
    Ok((event, size))
}

/// How the person is talking with the agent of `session_id`, as its record says.
fn interaction_of(tx: &Connection, session_id: &SessionId) -> Result<InteractionMode, StoreError> {
    let on: bool =
        tx.query_row("SELECT conversation IS NOT NULL FROM sessions WHERE id = ?", [session_id.as_str()], |row| {
            row.get(0)
        })?;
    Ok(if on { InteractionMode::Conversation } else { InteractionMode::Typing })
}

/// What the `interaction` column holds for `mode`: NULL for typing, as in every event from before it was recorded.
fn interaction_column(mode: InteractionMode) -> Option<String> {
    (!mode.is_typing()).then(|| text_of(&mode))
}
