//! Each prompt and compaction recorded in a chat, as one line, for the lists that don't need whole events: a chat's
//! outline, and what a workspace's title is worked out from.

use std::collections::HashMap;

use protocol::model::{CompactionLine, Place, PromptLine, Seq, SessionId, SettledAt, WorkspaceId};
use rusqlite::Connection;
use rusqlite::types::ValueRef;
use serde_json::Value;

use super::{Store, StoreError, lock};

impl Store {
    /// Every prompt recorded in the session, oldest first, each as one line, with where it was settled. It reads only
    /// prompts and the events that settle them, by their kind, and of each prompt only its text and how many files it
    /// carries, so a long session costs no more than its prompts' text.
    pub fn prompt_lines(&self, session_id: &SessionId) -> Result<Vec<PromptLine>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(
            "SELECT seq, json_extract(body, '$.text'), json_array_length(body, '$.attachments'), place FROM events \
             WHERE session_id = ? AND kind = 'user_prompt' ORDER BY seq",
        )?;
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
        let mut statement = conn.prepare_cached(
            "SELECT seq, place, json_extract(body, '$.outcome.by'), json_extract(body, '$.outcome.before_tokens'), \
             json_extract(body, '$.outcome.after_tokens') FROM events \
             WHERE session_id = ? AND kind = 'agent_compacted' AND subagent IS NULL \
             AND json_extract(body, '$.outcome.result') = 'compacted' ORDER BY seq",
        )?;
        let mut rows = statement.query([session_id.as_str()])?;
        let mut lines = Vec::new();
        while let Some(row) = rows.next()? {
            // Read as the event's body would be, so one who started it this build doesn't know is `Unknown`.
            let by = row.get::<_, Option<String>>(2)?.map_or(Value::Null, Value::String);
            let tokens = |ix| row.get::<_, Option<i64>>(ix).map(|tokens| tokens.and_then(|t| u64::try_from(t).ok()));
            lines.push(CompactionLine {
                seq: row.get::<_, i64>(0)?.cast_unsigned(),
                place: Place::new(row.get::<_, i64>(1)?.cast_unsigned()),
                by: serde_json::from_value(by).unwrap_or_default(),
                before_tokens: tokens(3)?,
                after_tokens: tokens(4)?,
            });
        }
        Ok(lines)
    }

    /// Every prompt recorded in the workspace's chats, oldest first whichever chat it was in, each as one line, read as
    /// [`Store::prompt_lines`] reads one chat's but without where it was settled, which nothing that reads these needs.
    /// A line's `seq` is its place in its own chat's log.
    pub fn workspace_prompt_lines(&self, workspace_id: &WorkspaceId) -> Result<Vec<PromptLine>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(
            "SELECT e.seq, json_extract(e.body, '$.text'), json_array_length(e.body, '$.attachments'), e.place \
             FROM sessions s JOIN events e ON e.session_id = s.id AND e.kind = 'user_prompt' \
             WHERE s.workspace_id = ? ORDER BY e.ts_ms, s.created_at_ms, e.seq",
        )?;
        lines_of(statement.query([workspace_id.as_str()])?)
    }
}

/// Where each prompt in the session was settled, by the prompt's `seq`: the last event that took or dropped it, as
/// [`SettledAt`] says. It reads only those events, by their kind, and of each only which prompt it names and where it
/// is.
fn settled_prompts(conn: &Connection, session_id: &SessionId) -> Result<HashMap<Seq, SettledAt>, StoreError> {
    let mut statement = conn.prepare_cached(
        "SELECT json_extract(body, '$.prompt'), seq, place FROM events \
         WHERE session_id = ? AND kind IN ('prompt_taken', 'prompt_dropped') ORDER BY seq",
    )?;
    let mut rows = statement.query([session_id.as_str()])?;
    let mut settled = HashMap::new();
    while let Some(row) = rows.next()? {
        let Some(prompt) = row.get::<_, Option<i64>>(0)? else { continue };
        let seq = row.get::<_, i64>(1)?.cast_unsigned();
        let place = Place::new(row.get::<_, i64>(2)?.cast_unsigned());
        // In order, so the last to name it stays.
        settled.insert(prompt.cast_unsigned(), SettledAt { seq, place });
    }
    Ok(settled)
}

/// The line for each row of a prompt's `seq`, text, how many files it carries, and place.
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
        let place = Place::new(row.get::<_, i64>(3)?.cast_unsigned());
        lines.push(PromptLine::new(seq, &text, files).placed(place));
    }
    Ok(lines)
}

#[cfg(test)]
mod tests;
