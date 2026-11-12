//! What recording something sent to a session's agent changes on the records, besides its log: the queue a prompt
//! comes out of, and when the person last sent the agent something, which is when they last did something in its
//! workspace too.

use protocol::event::{ApprovalOutcome, EventBody};
use protocol::model::{ClientMessageId, RunId, SessionId};
use rusqlite::{Connection, params};

use super::StoreError;
use super::queue::take_out_recorded;

/// Whether the person sent this, rather than the agent or the hub: a prompt, or an answer to a question or an
/// approval. Only the hub records them, as its own, so one with a run is the agent's, and a prompt taken out of the
/// queue as it's recorded was counted when it was queued.
fn sent_by_the_person(body: &EventBody) -> bool {
    matches!(
        body,
        EventBody::UserPrompt { .. }
            | EventBody::ApprovalResolved { outcome: ApprovalOutcome::Allowed | ApprovalOutcome::Denied, .. }
    )
}

/// Takes a prompt recorded under the ID of one in the queue out of the queue, and notes when the person last sent the
/// agent something, at `ts_ms`, both in the event's commit. Returns whether the event came out of the queue, and
/// whether the person sent it, as [`sent_by_the_person`] says.
pub(super) fn note_sent(
    tx: &Connection,
    session_id: &SessionId,
    ts_ms: i64,
    run: Option<RunId>,
    body: &EventBody,
    client_msg_id: Option<&ClientMessageId>,
) -> Result<(bool, bool), StoreError> {
    let dequeued = match (body, client_msg_id) {
        (EventBody::UserPrompt { .. }, Some(client_msg_id)) => take_out_recorded(tx, session_id, client_msg_id)?,
        _ => false,
    };
    let by_the_person = run.is_none() && !dequeued && sent_by_the_person(body);
    if by_the_person {
        note_active(tx, session_id, ts_ms)?;
    }
    Ok((dequeued, by_the_person))
}

/// Notes that the person sent session `id`'s agent something at `at_ms`, on the session's record and on its
/// workspace's, as part of the transaction that records what they sent.
pub(super) fn note_active(conn: &Connection, id: &SessionId, at_ms: i64) -> Result<(), StoreError> {
    conn.execute("UPDATE sessions SET user_active_at_ms = ?1 WHERE id = ?2", params![at_ms, id.as_str()])?;
    conn.execute(
        "UPDATE workspaces SET user_active_at_ms = ?1 WHERE id = (SELECT workspace_id FROM sessions WHERE id = ?2)",
        params![at_ms, id.as_str()],
    )?;
    Ok(())
}
