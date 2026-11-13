use super::super::sessions::queue_from;
use super::super::{Reading, Store, StoreError, lock};
use protocol::model::{Account, ClientMessageId, QueuedPrompt, SessionId};
use rusqlite::{Connection, OptionalExtension, params};

/// What asking to queue a prompt did.
pub enum Queued {
    /// It's queued.
    Done(Box<Queuing>),
    /// A prompt with its ID has been recorded, so it's been sent and can't change.
    AlreadySent,
}

/// What queuing a prompt left.
pub struct Queuing {
    /// The session as queuing left it, and whether anything changed.
    pub reading: Reading,
    /// The record of the account the session names, when queuing it changed when the account was last used.
    pub account: Option<Account>,
}

/// What asking to change a queued prompt did.
pub enum Changed {
    /// It's changed, with the session as that left it, and whether anything changed.
    Done(Box<Reading>),
    /// It's been recorded, so the agent has it as it was.
    AlreadySent,
    /// It's been taken out of the queue.
    NotQueued,
    /// The change would leave it saying nothing and carrying nothing.
    WouldBeEmpty,
}

impl Store {
    /// Puts `prompt` at the end of the queue. If a prompt with its ID is queued already, this is a retry, and changes
    /// nothing. A prompt that's been recorded is refused, so a retry that comes after it was sent doesn't queue it
    /// again.
    pub fn queue_prompt(&self, id: &SessionId, prompt: &QueuedPrompt) -> Result<Queued, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        if was_sent(&tx, id, &prompt.client_msg_id)? {
            return Ok(Queued::AlreadySent);
        }
        let mut queue = read_queue(&tx, id)?;
        if !queue.iter().any(|queued| queued.client_msg_id == prompt.client_msg_id) {
            queue.push(prompt.clone());
        }
        let changed = write_queue(&tx, id, &queue)?;
        // Queuing it is when the person sent it, rather than when the hub does.
        let account = if changed { super::super::sent::note_active(&tx, id, super::super::now_ms())? } else { None };
        let session = super::super::reading::session_on(&tx, id)?;
        tx.commit()?;
        Ok(Queued::Done(Box::new(Queuing { reading: Reading { session, changed }, account })))
    }

    /// Changes the text of the prompt queued under `client_msg_id`, which keeps its place and its files. One that's
    /// been sent or taken out is refused, so a change that comes too late isn't kept as if it would be sent.
    pub fn change_queued(
        &self,
        id: &SessionId,
        client_msg_id: &ClientMessageId,
        text: &str,
    ) -> Result<Changed, StoreError> {
        let conn = lock(&self.writer);
        if was_sent(&conn, id, client_msg_id)? {
            return Ok(Changed::AlreadySent);
        }
        let mut queue = read_queue(&conn, id)?;
        let Some(queued) = queue.iter_mut().find(|queued| queued.client_msg_id == *client_msg_id) else {
            return Ok(Changed::NotQueued);
        };
        text.clone_into(&mut queued.prompt.text);
        // Nothing's written if it's refused, so the queue keeps what was there.
        if !queued.prompt.says_or_carries() {
            return Ok(Changed::WouldBeEmpty);
        }
        Ok(Changed::Done(Box::new(rewrite_queue(&conn, id, &queue)?)))
    }

    /// Takes the prompt with this ID out of the queue, if it's there.
    pub fn unqueue_prompt(&self, id: &SessionId, client_msg_id: &ClientMessageId) -> Result<Reading, StoreError> {
        let conn = lock(&self.writer);
        let mut queue = read_queue(&conn, id)?;
        queue.retain(|queued| queued.client_msg_id != *client_msg_id);
        rewrite_queue(&conn, id, &queue)
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
pub(in super::super) fn take_out_recorded(
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

/// Whether a prompt with this ID has been recorded, which takes it out of the queue in the same commit.
fn was_sent(conn: &Connection, id: &SessionId, client_msg_id: &ClientMessageId) -> Result<bool, StoreError> {
    let sent: Option<i64> = conn
        .query_row(
            "SELECT seq FROM events WHERE session_id = ? AND client_msg_id = ?",
            params![id.as_str(), client_msg_id.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    Ok(sent.is_some())
}

fn read_queue(conn: &Connection, id: &SessionId) -> Result<Vec<QueuedPrompt>, StoreError> {
    let json: String = conn
        .query_row("SELECT queued FROM sessions WHERE id = ?", [id.as_str()], |row| row.get(0))
        .optional()?
        .ok_or_else(|| StoreError::NotFound { kind: "session", id: id.to_string() })?;
    Ok(queue_from(&json)?)
}

/// Saves the queue only if it changed, so the revision moves only then, and says whether it did.
fn write_queue(conn: &Connection, id: &SessionId, queue: &[QueuedPrompt]) -> Result<bool, StoreError> {
    let json = serde_json::to_string(queue)?;
    Ok(conn.execute("UPDATE sessions SET queued = ?1 WHERE id = ?2 AND queued <> ?1", params![json, id.as_str()])? > 0)
}

/// Saves the queue only if it changed, and reads the session back, as a change the person didn't send leaves it.
fn rewrite_queue(conn: &Connection, id: &SessionId, queue: &[QueuedPrompt]) -> Result<Reading, StoreError> {
    let changed = write_queue(conn, id, queue)?;
    Ok(Reading { session: super::super::reading::session_on(conn, id)?, changed })
}
