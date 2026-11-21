use super::super::{ByColumn, Columns, Store, StoreError, columns, lock, place_of, seq_of};
use protocol::model::{CompactionLine, PromptFrom, PromptLine, Seq, SessionId, SettledAt, Settlement, WorkspaceId};
use rusqlite::Connection;
use rusqlite::types::ValueRef;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;

columns! {
    /// What a prompt's line reads of its event, as [`lines_of`] reads it.
    struct PromptColumns {
        seq,
        text = "json_extract(body, '$.text')",
        files = "json_array_length(body, '$.attachments')",
        place,
        from = "json_extract(body, '$.from')",
    }
}

columns! {
    /// What a compaction's line reads of its event.
    struct CompactionColumns {
        seq,
        place,
        by = "json_extract(body, '$.outcome.by')",
        before_tokens = "json_extract(body, '$.outcome.before_tokens')",
        after_tokens = "json_extract(body, '$.outcome.after_tokens')",
    }
}

columns! {
    /// What [`settled_prompts`] reads of an event that took or dropped a prompt.
    struct SettlementColumns { prompt = "json_extract(body, '$.prompt')", seq, place, kind }
}

/// A chat's prompts, oldest first, as [`PromptColumns`] reads them.
pub(in crate::store) static PROMPTS: LazyLock<String> = LazyLock::new(|| {
    format!("SELECT {} FROM events WHERE session_id = ? AND kind = 'user_prompt' ORDER BY seq", PromptColumns::LIST)
});

/// The compactions of a chat's own conversation that compacted, oldest first, as [`CompactionColumns`] reads them.
pub(in crate::store) static COMPACTIONS: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM events \
         WHERE session_id = ? AND kind = 'agent_compacted' AND subagent IS NULL \
         AND json_extract(body, '$.outcome.result') = 'compacted' ORDER BY seq",
        CompactionColumns::LIST
    )
});

/// The events that took or dropped a prompt, oldest first, as [`SettlementColumns`] reads them. One kind at a time, so
/// each is read in order from the index of kinds and the two are merged, where `kind IN` would have SQLite read every
/// event of the session in order instead.
pub(in crate::store) static SETTLEMENTS: LazyLock<String> = LazyLock::new(|| {
    let columns = SettlementColumns::LIST;
    format!(
        "SELECT {columns} FROM events \
         WHERE session_id = ?1 AND kind = 'prompt_taken' AND json_extract(body, '$.prompt') IS NOT NULL \
         UNION ALL \
         SELECT {columns} FROM events \
         WHERE session_id = ?1 AND kind = 'prompt_dropped' AND json_extract(body, '$.prompt') IS NOT NULL \
         ORDER BY seq"
    )
});

impl Store {
    /// Every prompt recorded in the session, oldest first, each as one line, with where it was settled. It reads only
    /// prompts and the events that settle them, by their kind, and of each prompt only its text and how many files it
    /// carries, so a long session costs no more than its prompts' text.
    pub fn prompt_lines(&self, session_id: &SessionId) -> Result<Vec<PromptLine>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&PROMPTS)?;
        let lines = lines_of(statement.query([session_id.as_str()])?)?;
        let settled = settled_prompts(&conn, session_id)?;
        Ok(lines
            .into_iter()
            .map(|line| match settled.get(&line.seq()) {
                Some(at) => line.settled_at(*at),
                None => line,
            })
            .collect())
    }

    /// Every compaction of the agent's own conversation that compacted, oldest first, each as one line. It reads only
    /// compactions, by their kind, and of each only what its line says, so it leaves out the summary, which is long.
    pub fn compaction_lines(&self, session_id: &SessionId) -> Result<Vec<CompactionLine>, StoreError> {
        let conn = lock(&self.reader);
        let c = CompactionColumns::at(0);
        let mut statement = conn.prepare_cached(&COMPACTIONS)?;
        let mut rows = statement.query([session_id.as_str()])?;
        let mut lines = Vec::new();
        while let Some(row) = rows.next()? {
            // Read as the event's body would be, so one who started it this build doesn't know is `Unknown`.
            let by = row.at::<Option<String>>(c.by)?.map_or(Value::Null, Value::String);
            let tokens =
                |column| row.at::<Option<i64>>(column).map(|tokens| tokens.and_then(|t| u64::try_from(t).ok()));
            lines.push(CompactionLine {
                seq: seq_of(row, c.seq)?,
                place: place_of(row, c.place)?,
                by: serde_json::from_value(by).unwrap_or_default(),
                before_tokens: tokens(c.before_tokens)?,
                after_tokens: tokens(c.after_tokens)?,
            });
        }
        Ok(lines)
    }

    /// Every prompt recorded in the workspace's chats, oldest first whichever chat it was in, each as one line, read as
    /// [`Store::prompt_lines`] reads one chat's but without where it was settled, which nothing that reads these needs.
    /// A line's `seq` is in its own chat's log, so two chats' lines can have the same one.
    pub fn workspace_prompt_lines(&self, workspace_id: &WorkspaceId) -> Result<Vec<PromptLine>, StoreError> {
        let conn = lock(&self.reader);
        // The columns aren't qualified, as no column of `sessions` has their names.
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {} FROM sessions s JOIN events e ON e.session_id = s.id AND e.kind = 'user_prompt' \
             WHERE s.workspace_id = ? ORDER BY e.ts_ms, s.created_at_ms, e.seq",
            PromptColumns::LIST
        ))?;
        lines_of(statement.query([workspace_id.as_str()])?)
    }
}

/// Where each prompt in the session was settled, by the prompt's `seq`: the last event that took or dropped it, as
/// [`SettledAt`] says. It reads only those events, by their kind, and of each only which prompt it names and where it
/// is.
fn settled_prompts(conn: &Connection, session_id: &SessionId) -> Result<HashMap<Seq, SettledAt>, StoreError> {
    let c = SettlementColumns::at(0);
    let mut statement = conn.prepare_cached(&SETTLEMENTS)?;
    let mut rows = statement.query([session_id.as_str()])?;
    let mut settled = HashMap::new();
    while let Some(row) = rows.next()? {
        let (prompt, seq, place) = (seq_of(row, c.prompt)?, seq_of(row, c.seq)?, place_of(row, c.place)?);
        let how = if row.at::<String>(c.kind)? == "prompt_taken" { Settlement::Taken } else { Settlement::Dropped };
        // In order, so the last to name it stays.
        settled.insert(prompt, SettledAt { seq, place, how: Some(how) });
    }
    Ok(settled)
}

/// The line for each row of [`PromptColumns`]: a prompt's `seq`, text, how many files it carries, place, and who wrote
/// it, which is the person where the prompt doesn't say.
fn lines_of(mut rows: rusqlite::Rows<'_>) -> Result<Vec<PromptLine>, StoreError> {
    let c = PromptColumns::at(0);
    let mut lines = Vec::new();
    while let Some(row) = rows.next()? {
        let seq = seq_of(row, c.seq)?;
        // Borrowed rather than copied, since a pasted log can be long and only its start is kept. A JSON string
        // comes out as text, and a missing one as null.
        let text = match row.borrowed_at(c.text)? {
            ValueRef::Text(text) => String::from_utf8_lossy(text),
            ValueRef::Null | ValueRef::Integer(_) | ValueRef::Real(_) | ValueRef::Blob(_) => "".into(),
        };
        let files = u32::try_from(row.at::<Option<i64>>(c.files)?.unwrap_or(0)).unwrap_or(u32::MAX);
        let place = place_of(row, c.place)?;
        // An object, which comes out as its JSON, and only where the person didn't write it.
        let from = match row.at::<Option<String>>(c.from)? {
            Some(from) => serde_json::from_str(&from)?,
            None => PromptFrom::Person,
        };
        lines.push(PromptLine::new(seq, &text, files).placed(place).written_by(from));
    }
    Ok(lines)
}
