//! Each session's event log.

use protocol::event::{ApprovalOutcome, EventBody, SessionEvent};
use protocol::model::{Seq, SessionStatus};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::{Appended, Store, StoreError, lock, now_ms, text_of};

/// The event that closes an unanswered approval when nobody can answer it any more.
pub fn cancelled_approval(approval_id: String) -> EventBody {
    EventBody::ApprovalResolved { approval_id, outcome: ApprovalOutcome::Cancelled }
}

impl Store {
    /// `new_status` commits with the event, so nobody can see one without the other. A client that
    /// reads status after an event gets at least that event's status.
    pub fn append(
        &self,
        session_id: &str,
        body: &EventBody,
        raw: Option<&str>,
        client_msg_id: Option<&str>,
        new_status: Option<SessionStatus>,
    ) -> Result<Appended, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(client_msg_id) = client_msg_id {
            let existing = tx
                .query_row(
                    "SELECT seq, ts_ms, body FROM events WHERE session_id = ? AND client_msg_id = ?",
                    params![session_id, client_msg_id],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)),
                )
                .optional()?;
            if let Some((seq, ts_ms, body)) = existing {
                let body = serde_json::from_str(&body)?;
                let event = SessionEvent {
                    session_id: session_id.to_string(),
                    seq: seq.cast_unsigned(),
                    ts_ms,
                    body,
                    raw: None,
                };
                return Ok(Appended { event, duplicate: true });
            }
        }
        let last_seq: Option<i64> =
            tx.query_row("SELECT last_seq FROM sessions WHERE id = ?", [session_id], |row| row.get(0)).optional()?;
        let last_seq = last_seq.ok_or_else(|| StoreError::NotFound { kind: "session", id: session_id.to_string() })?;
        let seq = last_seq + 1;
        let ts_ms = now_ms();
        let encoded = serde_json::to_string(body)?;
        tx.execute(
            "INSERT INTO events (session_id, seq, ts_ms, kind, client_msg_id, body, raw) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![session_id, seq, ts_ms, body.kind(), client_msg_id, encoded, raw],
        )?;
        tx.execute(
            "UPDATE sessions SET last_seq = ?, updated_at_ms = ? WHERE id = ?",
            params![seq, ts_ms, session_id],
        )?;
        if let Some(status) = new_status {
            tx.execute("UPDATE sessions SET status = ? WHERE id = ?", params![text_of(&status), session_id])?;
        }
        tx.commit()?;
        let event = SessionEvent {
            session_id: session_id.to_string(),
            seq: seq.cast_unsigned(),
            ts_ms,
            body: body.clone(),
            raw: None,
        };
        Ok(Appended { event, duplicate: false })
    }

    /// Always returns at least one event if any exist at or after `from_seq`, however large.
    pub fn read_events(
        &self,
        session_id: &str,
        from_seq: Seq,
        limit: u32,
        max_bytes: u64,
        include_raw: bool,
    ) -> Result<Vec<SessionEvent>, StoreError> {
        let conn = lock(&self.reader);
        let raw_column = if include_raw { "raw" } else { "NULL" };
        let mut statement = conn.prepare_cached(&format!(
            "SELECT seq, ts_ms, body, {raw_column} FROM events WHERE session_id = ? AND seq >= ? ORDER BY seq LIMIT ?"
        ))?;
        let mut rows = statement.query(params![session_id, from_seq.cast_signed(), limit])?;
        let mut events = Vec::new();
        let mut bytes = 0u64;
        while let Some(row) = rows.next()? {
            let body: String = row.get(2)?;
            let raw: Option<String> = row.get(3)?;
            bytes += (body.len() + raw.as_ref().map_or(0, String::len)) as u64;
            events.push(SessionEvent {
                session_id: session_id.to_string(),
                seq: row.get::<_, i64>(0)?.cast_unsigned(),
                ts_ms: row.get(1)?,
                body: serde_json::from_str(&body)?,
                raw,
            });
            if bytes >= max_bytes {
                break;
            }
        }
        Ok(events)
    }

    /// Approvals requested and never resolved.
    pub fn open_approvals(&self, session_id: &str) -> Result<Vec<String>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn
            .prepare_cached(
                "SELECT body FROM events WHERE session_id = ? AND kind IN ('approval_requested', 'approval_resolved') ORDER BY seq",
            )
            ?;
        let rows = statement.query_map([session_id], |row| row.get::<_, String>(0))?;
        let mut open = Vec::new();
        for body in rows {
            let body: EventBody = serde_json::from_str(&body?)?;
            match body {
                EventBody::ApprovalRequested { approval_id, .. } => open.push(approval_id),
                EventBody::ApprovalResolved { approval_id, .. } => open.retain(|id| *id != approval_id),
                EventBody::UserPrompt { .. }
                | EventBody::AgentStarted { .. }
                | EventBody::AgentInit { .. }
                | EventBody::AgentState { .. }
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
