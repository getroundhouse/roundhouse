//! Each subagent's record, changed in the transaction that records the event that changes it.

use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Call, Rev, SessionId, Subagent, SubagentStatus, ToolUseId};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{Store, StoreError, current_rev, from_text, lock, rev_of, text_of};

const SUBAGENT_COLUMNS: &str = "session_id, call, kind, description, prompt, in_background, status, started_at_ms, \
     ended_at_ms, calls, latest_call, report, rev";

impl Store {
    /// The session's subagents, oldest first, read in one transaction with the revision they're as of.
    pub fn list_subagents(&self, session_id: &SessionId) -> Result<(Vec<Subagent>, Rev), StoreError> {
        let mut conn = lock(&self.reader);
        let tx = conn.transaction()?;
        let subagents = tx
            .prepare_cached(&format!(
                "SELECT {SUBAGENT_COLUMNS} FROM subagents WHERE session_id = ? ORDER BY started_at_ms, rowid"
            ))?
            .query_map([session_id.as_str()], subagent_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let rev = current_rev(&tx)?;
        tx.commit()?;
        Ok((subagents, rev))
    }

    pub fn get_subagent(&self, session_id: &SessionId, call: &ToolUseId) -> Result<Option<Subagent>, StoreError> {
        let conn = lock(&self.reader);
        let sql = format!("SELECT {SUBAGENT_COLUMNS} FROM subagents WHERE session_id = ? AND call = ?");
        Ok(conn.query_row(&sql, params![session_id.as_str(), call.as_str()], subagent_from_row).optional()?)
    }
}

const STARTED: &str = "INSERT OR IGNORE INTO subagents
     (session_id, call, kind, description, prompt, in_background, status, started_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?)";

const ENDED: &str = "UPDATE subagents SET status = ?, ended_at_ms = ?, report = ?
     WHERE session_id = ? AND call = ? AND status = ?";

/// Changes the records of the subagents `event` says something about, in the transaction that records it, and says
/// which. A subagent's record starts with the agent's word that it started, ends with its word that it ended, or with
/// the agent, which takes its subagents with it, and counts the calls in between from the subagent's own events.
pub(super) fn note_subagents(tx: &Connection, event: &SessionEvent) -> Result<Vec<ToolUseId>, StoreError> {
    let (session, at) = (event.session_id.as_str(), event.ts_ms);
    let working = text_of(&SubagentStatus::Working);
    let changed = |rows: usize, call: &ToolUseId| if rows == 0 { Vec::new() } else { vec![call.clone()] };
    match &event.body {
        EventBody::SubagentStarted { call, kind, description, prompt, in_background } => {
            let row = params![session, call.as_str(), kind, description, prompt, in_background, working, at];
            Ok(changed(tx.execute(STARTED, row)?, call))
        }
        EventBody::SubagentEnded { call, outcome, report } => {
            let status = text_of(&SubagentStatus::ended(*outcome));
            Ok(changed(tx.execute(ENDED, params![status, at, report, session, call.as_str(), working])?, call))
        }
        EventBody::AgentExited { .. } => {
            let mut statement = tx.prepare_cached(
                "UPDATE subagents SET status = ?, ended_at_ms = ? WHERE session_id = ? AND status = ? RETURNING call",
            )?;
            let stopped = text_of(&SubagentStatus::Stopped);
            let calls = statement.query_map(params![stopped, at, session, working], |row| row.get::<_, String>(0))?;
            Ok(calls.map(|call| call.map(ToolUseId::from)).collect::<rusqlite::Result<_>>()?)
        }
        EventBody::Assistant { blocks, .. } => {
            let Some(call) = &event.subagent else { return Ok(Vec::new()) };
            let made: Vec<Call> = blocks.iter().filter_map(call_in).collect();
            let Some(latest) = made.last() else { return Ok(Vec::new()) };
            let rows = tx.execute(
                "UPDATE subagents SET calls = calls + ?, latest_call = ? WHERE session_id = ? AND call = ?",
                params![made.len(), serde_json::to_string(latest)?, session, call.as_str()],
            )?;
            Ok(changed(rows, call))
        }
        EventBody::UserPrompt { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentUser { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
        | EventBody::AgentOther { .. }
        | EventBody::Unknown => Ok(Vec::new()),
    }
}

fn call_in(block: &ContentBlock) -> Option<Call> {
    let ContentBlock::ToolUse { id, name, input } = block else { return None };
    Some(Call { id: id.as_str().into(), name: name.clone(), input: input.clone() })
}

fn subagent_from_row(row: &Row<'_>) -> rusqlite::Result<Subagent> {
    Ok(Subagent {
        session_id: row.get::<_, String>(0)?.into(),
        call: row.get::<_, String>(1)?.into(),
        kind: row.get(2)?,
        description: row.get(3)?,
        prompt: row.get(4)?,
        in_background: row.get(5)?,
        status: from_text(row.get(6)?)?,
        started_at_ms: row.get(7)?,
        ended_at_ms: row.get(8)?,
        calls: row.get(9)?,
        // Written only by this hub, as JSON. One it can't read is none it knows of.
        latest_call: row.get::<_, Option<String>>(10)?.and_then(|call| serde_json::from_str(&call).ok()),
        report: row.get(11)?,
        rev: rev_of(row, 12)?,
    })
}

#[cfg(test)]
mod tests;
