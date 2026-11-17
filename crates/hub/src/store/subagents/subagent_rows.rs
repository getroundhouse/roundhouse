use super::super::{Store, StoreError, current_rev, from_text, json_at, lock, rev_of, text_of};
use super::recent;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Call, ModelName, RecentWork, Rev, SessionId, Subagent, SubagentId, SubagentStatus};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, Row, params};

const SUBAGENT_COLUMNS: &str = "session_id, call, kind, description, prompt, in_background, status, started_at_ms, \
     ended_at_ms, calls, latest_call, report, rev, recent, model, context_tokens, id";

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

    pub fn get_subagent(&self, session_id: &SessionId, subagent: &SubagentId) -> Result<Option<Subagent>, StoreError> {
        let conn = lock(&self.reader);
        let sql = format!("SELECT {SUBAGENT_COLUMNS} FROM subagents WHERE session_id = ? AND id = ?");
        Ok(conn.query_row(&sql, params![session_id.as_str(), subagent.as_str()], subagent_from_row).optional()?)
    }

    /// The subagent `named` names in `session_id`: the one with that ID, or else the one the call it names started,
    /// since a client from before subagents had IDs of their own names each by its call. One that names neither is
    /// taken as it is, which reads no events, as it did before.
    pub fn subagent_named(&self, session_id: &SessionId, named: &SubagentId) -> Result<SubagentId, StoreError> {
        Ok(self.recorded_subagent(session_id, named)?.unwrap_or_else(|| named.clone()))
    }

    /// The subagent `named` names in `session_id`, as [`Store::subagent_named`] finds it, if there's a record of it.
    pub fn recorded_subagent(
        &self,
        session_id: &SessionId,
        named: &SubagentId,
    ) -> Result<Option<SubagentId>, StoreError> {
        let conn = lock(&self.reader);
        let sql = "SELECT id FROM subagents WHERE session_id = ?1 AND (id = ?2 OR call = ?2) ORDER BY id = ?2 DESC";
        let found = conn.query_row(sql, params![session_id.as_str(), named.as_str()], |row| row.get::<_, String>(0));
        Ok(found.optional()?.map(SubagentId::from))
    }
}

// One that starts again after it ended, as Claude's does when it's messaged, works again, under the call that first
// started it, with what it was first asked, from when it started again, so how long it's at it isn't the time it sat
// finished, and its report is the one it gives next. Another call can't start one that's already a subagent's.
const STARTED: &str = "INSERT INTO subagents
     (session_id, id, call, kind, description, prompt, in_background, status, started_at_ms)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT (session_id, id) DO UPDATE SET status = excluded.status, in_background = excluded.in_background,
         started_at_ms = excluded.started_at_ms, ended_at_ms = NULL, report = NULL
         WHERE status <> excluded.status
     ON CONFLICT DO NOTHING";

// What it's done lately is only shown while it works. An end that counts no tokens leaves the count it had.
const ENDED: &str = "UPDATE subagents SET status = ?, ended_at_ms = ?, report = ?, recent = NULL,
         context_tokens = coalesce(?, context_tokens)
     WHERE session_id = ? AND id = ? AND status = ?";

// Only while it works, since its end has the last word, and only a count that differs, so its record keeps its
// revision and isn't sent again for one that says the same.
const CONTEXT: &str = "UPDATE subagents SET context_tokens = ?1
     WHERE session_id = ?2 AND id = ?3 AND status = ?4 AND context_tokens IS NOT ?1";

/// Changes the records of the subagents `event` says something about, in the transaction that records it, and says
/// which. A subagent's record starts with the agent's word that it started, ends with its word that it ended, or with
/// the agent, which takes its subagents with it, works again if the agent says it started again, and counts the calls in between from the subagent's own events,
/// which also say what it's done lately, and its replies the model it runs on. How many tokens its conversation
/// holds is the agent's word too, as it works and as it ends.
pub(in super::super) fn note_subagents(tx: &Connection, event: &SessionEvent) -> Result<Vec<SubagentId>, StoreError> {
    let (session, at) = (event.session_id.as_str(), event.ts_ms);
    let working = text_of(&SubagentStatus::Working);
    match &event.body {
        EventBody::SubagentStarted { call, kind, description, prompt, in_background, .. } => {
            let Some(subagent) = event.body.subagent_started() else { return Ok(Vec::new()) };
            let (id, call) = (subagent.as_str(), call.as_str());
            let row = params![session, id, call, kind, description, prompt, in_background, working, at];
            Ok(changed(tx.execute(STARTED, row)?, &subagent))
        }
        EventBody::SubagentContext { subagent, tokens } => {
            let row = params![tokens.cast_signed(), session, subagent.as_str(), working];
            Ok(changed(tx.prepare_cached(CONTEXT)?.execute(row)?, subagent))
        }
        EventBody::SubagentEnded { subagent, outcome, report, context_tokens } => {
            let status = text_of(&SubagentStatus::ended(*outcome));
            let tokens = context_tokens.map(u64::cast_signed);
            let row = params![status, at, report, tokens, session, subagent.as_str(), working];
            Ok(changed(tx.execute(ENDED, row)?, subagent))
        }
        EventBody::AgentExited { .. } => {
            let mut statement = tx.prepare_cached(
                "UPDATE subagents SET status = ?, ended_at_ms = ?, recent = NULL WHERE session_id = ? AND status = ?
                 RETURNING id",
            )?;
            let stopped = text_of(&SubagentStatus::Stopped);
            let ids = statement.query_map(params![stopped, at, session, working], |row| row.get::<_, String>(0))?;
            Ok(ids.map(|id| id.map(SubagentId::from)).collect::<rusqlite::Result<_>>()?)
        }
        // Its calls and what it wrote come from its side, and what came back for its calls from the user's.
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => match &event.subagent {
            Some(subagent) => note_work(tx, event, subagent, blocks),
            None => Ok(Vec::new()),
        },
        EventBody::UserPrompt { .. }
        | EventBody::CompactionAsked { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::OwnTurn(_)
        | EventBody::AgentLimits { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::Notice { .. }
        | EventBody::RequestFailed { .. }
        | EventBody::AgentMode { .. }
        | EventBody::AgentOther { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::AgentCompacted { .. }
        | EventBody::CacheExpired { .. }
        | EventBody::Unknown => Ok(Vec::new()),
    }
}

/// `subagent`, if `rows` of the records were changed, which is at most its own.
fn changed(rows: usize, subagent: &SubagentId) -> Vec<SubagentId> {
    if rows == 0 { Vec::new() } else { vec![subagent.clone()] }
}

/// What `event`, one of `subagent`'s own, with `blocks`, adds to its record: the calls it made, what it's done lately,
/// and from a reply, the model it runs on.
fn note_work(
    tx: &Connection,
    event: &SessionEvent,
    subagent: &SubagentId,
    blocks: &[ContentBlock],
) -> Result<Vec<SubagentId>, StoreError> {
    let session = event.session_id.as_str();
    let model = note_model(tx, session, subagent, &event.body)?;
    let made: Vec<Call> = blocks.iter().filter_map(Call::of).collect();
    let latest = made.last().map(serde_json::to_string).transpose()?;
    let mut recent = recent_work(tx, session, subagent)?;
    let wrote = matches!(event.body, EventBody::Assistant { .. });
    let added = recent.as_mut().is_some_and(|work| recent::add(work, blocks, wrote));
    if made.is_empty() && !added {
        return Ok(changed(model, subagent));
    }
    let recent = recent.filter(|_| added).map(|work| serde_json::to_string(&work)).transpose()?;
    let rows = tx.execute(
        "UPDATE subagents SET calls = calls + ?, latest_call = coalesce(?, latest_call),
             recent = coalesce(?, recent) WHERE session_id = ? AND id = ?",
        params![made.len(), latest, recent, session, subagent.as_str()],
    )?;
    Ok(changed(rows + model, subagent))
}

/// Notes the model a reply from `subagent` came from, if it names one, and says how many records that changed. Most of
/// its replies name the model the last one did, which changes nothing, so its record keeps its revision and isn't sent
/// again for it.
fn note_model(tx: &Connection, session: &str, subagent: &SubagentId, body: &EventBody) -> Result<usize, StoreError> {
    let EventBody::Assistant { model: Some(model), .. } = body else { return Ok(0) };
    let Ok(model) = ModelName::try_from(model.clone()) else { return Ok(0) };
    let sql = "UPDATE subagents SET model = ?1 WHERE session_id = ?2 AND id = ?3 AND model IS NOT ?1";
    Ok(tx.prepare_cached(sql)?.execute(params![model.as_str(), session, subagent.as_str()])?)
}

/// What `subagent` has done lately, if it's still working, since that's only kept while it works.
fn recent_work(tx: &Connection, session: &str, subagent: &SubagentId) -> Result<Option<RecentWork>, StoreError> {
    let working = text_of(&SubagentStatus::Working);
    let recent = tx
        .prepare_cached("SELECT recent FROM subagents WHERE session_id = ? AND id = ? AND status = ?")?
        .query_row(params![session, subagent.as_str(), working], |row| json_at::<RecentWork>(row, 0))
        .optional()?;
    // Written only by this hub, as JSON. What it can't read is nothing it knows of.
    Ok(recent.map(Option::unwrap_or_default))
}

fn subagent_from_row(row: &Row<'_>) -> rusqlite::Result<Subagent> {
    Ok(Subagent {
        session_id: row.get::<_, String>(0)?.into(),
        id: Some(row.get::<_, String>(16)?.into()),
        call: row.get::<_, String>(1)?.into(),
        kind: text_at(row, 2)?,
        description: text_at(row, 3)?,
        prompt: text_at(row, 4)?,
        in_background: row.get(5)?,
        // Written only by this hub, from a name that named a model. One that doesn't is none it knows of.
        model: row.get::<_, Option<String>>(14)?.and_then(|model| ModelName::try_from(model).ok()),
        status: from_text(row.get(6)?)?,
        started_at_ms: row.get(7)?,
        ended_at_ms: row.get(8)?,
        context_tokens: row.get::<_, Option<i64>>(15)?.map(i64::cast_unsigned),
        calls: row.get(9)?,
        // Written only by this hub, as JSON. One it can't read is none it knows of.
        latest_call: json_at(row, 10)?,
        report: text_at(row, 11)?,
        rev: rev_of(row, 12)?,
        // As in `recent_work`.
        recent: json_at(row, 13)?.unwrap_or_default(),
        // This hub keeps a record's calls only in the agent's shape, so it sends what a hub from before sends.
        lately: None,
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
