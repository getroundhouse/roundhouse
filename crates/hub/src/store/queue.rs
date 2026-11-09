//! Prompts waiting their turn, kept on their session's record.

use protocol::model::{ClientMessageId, QueuedPrompt, SessionId};
use rusqlite::{Connection, OptionalExtension, params};

use super::sessions::queue_from;
use super::{Reading, Store, StoreError, lock};

#[cfg(test)]
mod tests;

/// What asking to queue a prompt did.
pub enum Queued {
    /// It's queued, with the session as that left it, and whether anything changed.
    Done(Box<Reading>),
    /// A prompt with its ID has been recorded, so it's been sent and can't change.
    AlreadySent,
}

impl Store {
    /// Puts `prompt` at the end of the queue, or changes the text of the one with its ID, which keeps its place. A
    /// prompt that's been recorded is refused, so a change that comes after it was sent isn't kept as if it would be.
    pub fn queue_prompt(&self, id: &SessionId, prompt: &QueuedPrompt) -> Result<Queued, StoreError> {
        let conn = lock(&self.writer);
        let sent: Option<i64> = conn
            .query_row(
                "SELECT seq FROM events WHERE session_id = ? AND client_msg_id = ?",
                params![id.as_str(), prompt.client_msg_id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        if sent.is_some() {
            return Ok(Queued::AlreadySent);
        }
        let mut queue = read_queue(&conn, id)?;
        match queue.iter_mut().find(|queued| queued.client_msg_id == prompt.client_msg_id) {
            Some(queued) => queued.text.clone_from(&prompt.text),
            None => queue.push(prompt.clone()),
        }
        Ok(Queued::Done(Box::new(write_queue(&conn, id, &queue)?)))
    }

    /// Takes the prompt with this ID out of the queue, if it's there.
    pub fn unqueue_prompt(&self, id: &SessionId, client_msg_id: &ClientMessageId) -> Result<Reading, StoreError> {
        let conn = lock(&self.writer);
        let mut queue = read_queue(&conn, id)?;
        queue.retain(|queued| queued.client_msg_id != *client_msg_id);
        write_queue(&conn, id, &queue)
    }

    /// The prompt to send next, if any.
    pub fn first_queued(&self, id: &SessionId) -> Result<Option<QueuedPrompt>, StoreError> {
        Ok(read_queue(&lock(&self.reader), id)?.into_iter().next())
    }

    /// The prompt queued under this ID, if it still is.
    pub fn queued_prompt(
        &self,
        id: &SessionId,
        client_msg_id: &ClientMessageId,
    ) -> Result<Option<QueuedPrompt>, StoreError> {
        let queue = read_queue(&lock(&self.reader), id)?;
        Ok(queue.into_iter().find(|queued| queued.client_msg_id == *client_msg_id))
    }

    /// Every session with a prompt in its queue.
    pub fn sessions_with_queues(&self) -> Result<Vec<SessionId>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare("SELECT id FROM sessions WHERE queued <> '[]'")?;
        let ids = statement.query_map([], |row| Ok(SessionId::from(row.get::<_, String>(0)?)))?;
        Ok(ids.collect::<rusqlite::Result<_>>()?)
    }
}

/// Takes the prompt with this ID out of the session's queue, as part of the transaction that records it, and says
/// whether it was there.
pub(super) fn take_out_recorded(
    conn: &Connection,
    id: &SessionId,
    client_msg_id: &ClientMessageId,
) -> Result<bool, StoreError> {
    let queue = read_queue(conn, id)?;
    let left: Vec<&QueuedPrompt> = queue.iter().filter(|queued| queued.client_msg_id != *client_msg_id).collect();
    if left.len() == queue.len() {
        return Ok(false);
    }
    conn.execute("UPDATE sessions SET queued = ? WHERE id = ?", params![serde_json::to_string(&left)?, id.as_str()])?;
    Ok(true)
}

fn read_queue(conn: &Connection, id: &SessionId) -> Result<Vec<QueuedPrompt>, StoreError> {
    let json: String = conn
        .query_row("SELECT queued FROM sessions WHERE id = ?", [id.as_str()], |row| row.get(0))
        .optional()?
        .ok_or_else(|| StoreError::NotFound { kind: "session", id: id.to_string() })?;
    Ok(queue_from(&json)?)
}

/// Saves the queue only if it changed, so the revision moves only then, and reads the session back.
fn write_queue(conn: &Connection, id: &SessionId, queue: &[QueuedPrompt]) -> Result<Reading, StoreError> {
    let json = serde_json::to_string(queue)?;
    let changed =
        conn.execute("UPDATE sessions SET queued = ?1 WHERE id = ?2 AND queued <> ?1", params![json, id.as_str()])? > 0;
    let session = super::reading::session_on(conn, id)?;
    Ok(Reading { session, changed })
}
