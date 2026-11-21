use super::super::{ByColumn, Columns, Store, StoreError, columns, lock};
use protocol::event::EventBody;
use protocol::model::{Seq, SessionId};
use rusqlite::{Connection, params};

/// Notes what the event recorded at `seq` does to the prompts the agent has yet to take, as part of the transaction
/// that records it, and returns them if it changed them. A prompt waits until a `PromptTaken` or `PromptDropped` names
/// it.
pub(in super::super) fn note_prompts(
    conn: &Connection,
    id: &SessionId,
    seq: Seq,
    body: &EventBody,
) -> Result<Option<Vec<Seq>>, StoreError> {
    let settled = if let EventBody::PromptTaken { prompt } | EventBody::PromptDropped { prompt } = body {
        Some(*prompt)
    } else if matches!(body, EventBody::UserPrompt { .. } | EventBody::CompactionAsked { .. }) {
        None
    } else {
        return Ok(None);
    };
    let json: String = conn.query_row("SELECT untaken FROM sessions WHERE id = ?", [id.as_str()], |row| row.get(0))?;
    let mut untaken: Vec<Seq> = serde_json::from_str(&json)?;
    match settled {
        None => untaken.push(seq),
        Some(prompt) if untaken.contains(&prompt) => untaken.retain(|waiting| *waiting != prompt),
        // Settled already, as a prompt dropped with a lost worker and then taken by the agent after it.
        Some(_) => return Ok(None),
    }
    conn.execute(
        "UPDATE sessions SET untaken = ? WHERE id = ?",
        params![serde_json::to_string(&untaken)?, id.as_str()],
    )?;
    Ok(Some(untaken))
}

impl Store {
    /// Every session with prompts its agent has yet to take, and those prompts, oldest first.
    pub fn untaken_prompts(&self) -> Result<Vec<(SessionId, Vec<Seq>)>, StoreError> {
        let conn = lock(&self.reader);
        columns! {
            struct UntakenColumns { id, untaken }
        }
        let c = UntakenColumns::at(0);
        let mut statement =
            conn.prepare(&format!("SELECT {} FROM sessions WHERE untaken <> '[]'", UntakenColumns::LIST))?;
        let rows = statement
            .query_map([], |row| Ok((SessionId::from(row.at::<String>(c.id)?), row.at::<String>(c.untaken)?)))?;
        let mut untaken = Vec::new();
        for row in rows {
            let (id, json) = row?;
            untaken.push((id, serde_json::from_str(&json)?));
        }
        Ok(untaken)
    }
}
