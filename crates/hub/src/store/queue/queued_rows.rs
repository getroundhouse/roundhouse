use super::super::proposals::take_back_answer_on;
use super::super::sessions::queue_from;
use super::super::{Reading, Store, StoreError, lock, seq_of};
use protocol::model::{Account, ClientMessageId, Proposal, QueuedPrompt, Seq, SessionId};
use protocol::prompt::PromptFrom;
use rusqlite::{Connection, OptionalExtension, params};

/// What asking to queue a prompt did.
pub enum Queued {
    /// It's queued.
    Done(Box<Queuing>),
    /// A prompt with its ID has been recorded, so it's been sent and can't change.
    AlreadySent,
}

impl Queued {
    /// What queuing it left, if it was queued.
    pub fn queuing(self) -> Option<Queuing> {
        match self {
            Queued::Done(queuing) => Some(*queuing),
            Queued::AlreadySent => None,
        }
    }
}

/// What taking a prompt out of the queue left.
pub struct Unqueued {
    /// The session as it left it, and whether anything changed.
    pub reading: Reading,
    /// The proposal whose answer it took back, when the prompt was the one telling the agent of it.
    pub taken_back: Option<Proposal>,
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
    /// The hub wrote it, such as to tell the agent the answer to a proposal, so its words are the hub's to say.
    NotThePersons,
}

impl Store {
    /// Where the prompt sent under `client_msg_id` is recorded, if it has been, so a request sent again can be told
    /// what the first one did.
    pub fn sent_at(&self, id: &SessionId, client_msg_id: &ClientMessageId) -> Result<Option<Seq>, StoreError> {
        sent_at(&lock(&self.reader), id, client_msg_id)
    }

    /// Whether a prompt with this ID is queued, or has been sent, so queuing it again is a retry.
    pub fn queued_or_sent(&self, id: &SessionId, client_msg_id: &ClientMessageId) -> Result<bool, StoreError> {
        let conn = lock(&self.reader);
        let queued = read_queue(&conn, id)?.iter().any(|queued| queued.client_msg_id == *client_msg_id);
        Ok(queued || was_sent(&conn, id, client_msg_id)?)
    }

    /// Puts `prompt` at the end of the queue. If a prompt with its ID is queued already, this is a retry, and changes
    /// nothing. A prompt that's been recorded is refused, so a retry that comes after it was sent doesn't queue it
    /// again.
    pub fn queue_prompt(&self, id: &SessionId, prompt: &QueuedPrompt) -> Result<Queued, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let queued = queue_on(&tx, id, prompt)?;
        tx.commit()?;
        Ok(queued)
    }

    /// Changes the text of the prompt queued under `client_msg_id`, which keeps its place and its files. One that's
    /// been sent or taken out is refused, so a change that comes too late isn't kept as if it would be sent, and so is
    /// one the hub wrote, which would otherwise carry the person's words as the hub's, whatever a client offers.
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
        if !queued.from.is_person() {
            return Ok(Changed::NotThePersons);
        }
        text.clone_into(&mut queued.prompt.text);
        // Nothing's written if it's refused, so the queue keeps what was there.
        if !queued.prompt.says_or_carries() {
            return Ok(Changed::WouldBeEmpty);
        }
        Ok(Changed::Done(Box::new(rewrite_queue(&conn, id, &queue)?)))
    }

    /// Takes the prompt with this ID out of the queue, if it's there. One telling the agent the answer to a proposal
    /// takes the answer back with it, in the same commit, so no answer is kept that its agent won't be told of.
    pub fn unqueue_prompt(&self, id: &SessionId, client_msg_id: &ClientMessageId) -> Result<Unqueued, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let mut queue = read_queue(&tx, id)?;
        let taken = queue.iter().position(|queued| queued.client_msg_id == *client_msg_id).map(|at| queue.remove(at));
        let taken_back = match taken.map(|taken| taken.from) {
            Some(PromptFrom::ProposalResponse { proposal, revision }) => {
                Some(take_back_answer_on(&tx, &proposal, revision)?)
            }
            _ => None,
        };
        let reading = rewrite_queue(&tx, id, &queue)?;
        tx.commit()?;
        Ok(Unqueued { reading, taken_back })
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

/// Puts `prompt` at the end of the session `id`'s queue, as [`Store::queue_prompt`] does, on `conn`, so what else
/// is written with it is kept with it, or not at all.
pub(in super::super) fn queue_on(
    conn: &Connection,
    id: &SessionId,
    prompt: &QueuedPrompt,
) -> Result<Queued, StoreError> {
    if was_sent(conn, id, &prompt.client_msg_id)? {
        return Ok(Queued::AlreadySent);
    }
    let mut queue = read_queue(conn, id)?;
    if !queue.iter().any(|queued| queued.client_msg_id == prompt.client_msg_id) {
        queue.push(prompt.clone());
    }
    let changed = write_queue(conn, id, &queue)?;
    // Queuing it is when the person sent it, rather than when the hub does.
    let account = if changed { super::super::sent::note_active(conn, id, super::super::now_ms())? } else { None };
    let session = super::super::reading::session_on(conn, id)?;
    Ok(Queued::Done(Box::new(Queuing { reading: Reading { session, changed }, account })))
}

/// Whether a prompt with this ID has been recorded, which takes it out of the queue in the same commit.
fn was_sent(conn: &Connection, id: &SessionId, client_msg_id: &ClientMessageId) -> Result<bool, StoreError> {
    Ok(sent_at(conn, id, client_msg_id)?.is_some())
}

/// Where the prompt with this ID is recorded, if it has been.
fn sent_at(conn: &Connection, id: &SessionId, client_msg_id: &ClientMessageId) -> Result<Option<Seq>, StoreError> {
    let sent = conn
        .query_row(
            "SELECT seq FROM events WHERE session_id = ? AND client_msg_id = ?",
            params![id.as_str(), client_msg_id.as_str()],
            |row| seq_of(row, 0),
        )
        .optional()?;
    Ok(sent)
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
