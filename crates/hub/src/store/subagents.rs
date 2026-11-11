//! Each subagent's record, changed in the transaction that records the event that changes it.

use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Call, RecentWork, Rev, SessionId, Subagent, SubagentStatus, ToolUseId};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{Store, StoreError, current_rev, from_text, lock, rev_of, text_of};

mod recent;

const SUBAGENT_COLUMNS: &str = "session_id, call, kind, description, prompt, in_background, status, started_at_ms, \
     ended_at_ms, calls, latest_call, report, rev, recent";

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

// What it's done lately is only shown while it works.
const ENDED: &str = "UPDATE subagents SET status = ?, ended_at_ms = ?, report = ?, recent = NULL
     WHERE session_id = ? AND call = ? AND status = ?";

/// Changes the records of the subagents `event` says something about, in the transaction that records it, and says
/// which. A subagent's record starts with the agent's word that it started, ends with its word that it ended, or with
/// the agent, which takes its subagents with it, and counts the calls in between from the subagent's own events,
/// which also say what it's done lately.
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
                "UPDATE subagents SET status = ?, ended_at_ms = ?, recent = NULL WHERE session_id = ? AND status = ?
                 RETURNING call",
            )?;
            let stopped = text_of(&SubagentStatus::Stopped);
            let calls = statement.query_map(params![stopped, at, session, working], |row| row.get::<_, String>(0))?;
            Ok(calls.map(|call| call.map(ToolUseId::from)).collect::<rusqlite::Result<_>>()?)
        }
        // Its calls and what it wrote come from its side, and what came back for its calls from the user's.
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
            let Some(call) = &event.subagent else { return Ok(Vec::new()) };
            let made: Vec<Call> = blocks.iter().filter_map(call_in).collect();
            let latest = made.last().map(serde_json::to_string).transpose()?;
            let mut recent = recent_work(tx, session, call)?;
            let wrote = matches!(event.body, EventBody::Assistant { .. });
            let added = recent.as_mut().is_some_and(|work| recent::add(work, blocks, wrote));
            if made.is_empty() && !added {
                return Ok(Vec::new());
            }
            let recent = recent.filter(|_| added).map(|work| serde_json::to_string(&work)).transpose()?;
            let rows = tx.execute(
                "UPDATE subagents SET calls = calls + ?, latest_call = coalesce(?, latest_call),
                     recent = coalesce(?, recent) WHERE session_id = ? AND call = ?",
                params![made.len(), latest, recent, session, call.as_str()],
            )?;
            Ok(changed(rows, call))
        }
        EventBody::UserPrompt { .. }
        | EventBody::CompactionAsked { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentLimits { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
        | EventBody::AgentOther { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::AgentCompacted { .. }
        | EventBody::Unknown => Ok(Vec::new()),
    }
}

/// What the subagent `call` started has done lately, if it's still working, since that's only kept while it works.
fn recent_work(tx: &Connection, session: &str, call: &ToolUseId) -> Result<Option<RecentWork>, StoreError> {
    let working = text_of(&SubagentStatus::Working);
    let recent = tx
        .prepare_cached("SELECT recent FROM subagents WHERE session_id = ? AND call = ? AND status = ?")?
        .query_row(params![session, call.as_str(), working], |row| row.get::<_, Option<String>>(0))
        .optional()?;
    Ok(recent.map(|recent| read_recent(recent.as_deref())))
}

/// Written only by this hub, as JSON. What it can't read is nothing it knows of.
fn read_recent(json: Option<&str>) -> RecentWork {
    json.and_then(|json| serde_json::from_str(json).ok()).unwrap_or_default()
}

fn call_in(block: &ContentBlock) -> Option<Call> {
    let ContentBlock::ToolUse { id, name, input } = block else { return None };
    Some(Call { id: id.as_str().into(), name: name.clone(), input: input.clone() })
}

fn subagent_from_row(row: &Row<'_>) -> rusqlite::Result<Subagent> {
    Ok(Subagent {
        session_id: row.get::<_, String>(0)?.into(),
        call: row.get::<_, String>(1)?.into(),
        kind: text_at(row, 2)?,
        description: text_at(row, 3)?,
        prompt: text_at(row, 4)?,
        in_background: row.get(5)?,
        status: from_text(row.get(6)?)?,
        started_at_ms: row.get(7)?,
        ended_at_ms: row.get(8)?,
        calls: row.get(9)?,
        // Written only by this hub, as JSON. One it can't read is none it knows of.
        latest_call: row.get::<_, Option<String>>(10)?.and_then(|call| serde_json::from_str(&call).ok()),
        report: text_at(row, 11)?,
        rev: rev_of(row, 12)?,
        recent: read_recent(row.get::<_, Option<String>>(13)?.as_deref()),
    })
}

/// What the CLI said, as the adapter would have kept it. The records filled in from logs from before them were read
/// from the CLI's own lines, where half of a surrogate pair, which JavaScript writes, comes out of SQLite as three
/// bytes that aren't UTF-8. The adapter repairs a line before reading it, making each half U+FFFD, and so does this,
/// so a record can't fail its session's list.
fn text_at(row: &Row<'_>, column: usize) -> rusqlite::Result<Option<String>> {
    let ValueRef::Text(mut bytes) = row.get_ref(column)? else { return row.get(column) };
    let mut text = String::with_capacity(bytes.len());
    loop {
        match std::str::from_utf8(bytes) {
            Ok(rest) => {
                text.push_str(rest);
                return Ok(Some(text));
            }
            Err(e) => {
                let (valid, rest) = bytes.split_at(e.valid_up_to());
                text.push_str(&String::from_utf8_lossy(valid));
                text.push(char::REPLACEMENT_CHARACTER);
                let half = matches!(rest, [0xED, 0xA0..=0xBF, 0x80..=0xBF, ..]);
                let skipped = if half { 3 } else { e.error_len().unwrap_or(rest.len()) };
                bytes = rest.get(skipped..).unwrap_or_default();
            }
        }
    }
}

#[cfg(test)]
mod tests;
