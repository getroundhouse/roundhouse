//! What an event changes about its session's record, which commits with it.

use super::super::{StoreError, seq_column, text_of};
use protocol::event::{AgentTask, TurnOutcome};
use protocol::model::{RunId, Seq, SessionId, SessionStatus};
use rusqlite::{Connection, params};

/// What an event changes about where a session's agent has got to, which commits with the event that brings it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusChange {
    pub status: SessionStatus,
    pub turn: TurnEdge,
    /// What it's running in the background now, if the event changes that.
    pub background_tasks: Option<Vec<AgentTask>>,
}

/// What a change of status does to the session's turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnEdge {
    /// The event starts a turn, so its time becomes when the newest turn started.
    Starts,
    /// The event ends a turn, so it becomes the session's newest reply, which ended as it says, if it says.
    Ends(Option<TurnOutcome>),
    /// It does neither, such as the agent stopping to ask, or going away mid-turn.
    Neither,
}

impl From<SessionStatus> for StatusChange {
    fn from(status: SessionStatus) -> Self {
        Self { status, turn: TurnEdge::Neither, background_tasks: None }
    }
}

/// What an event changes about its session, which commits with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionChange {
    pub status: Option<StatusChange>,
    /// Whether the queue is held from here on.
    pub queue_held: Option<bool>,
    /// The run whose worker numbered what's recorded, and its number, which is the last recorded from that worker.
    pub recorded: Option<(RunId, u64)>,
    /// A current run opened its conversation, whose identity commits with its initialization.
    pub initialized: Option<Initialized>,
    /// What the current run said it claims, which the record holds until it exits.
    pub claims: Option<protocol::model::RunCapabilities>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Initialized {
    pub conversation: Option<protocol::model::AgentSessionId>,
}

impl SessionChange {
    /// Whether nothing on the session's record changes. Which number from its worker it has recorded isn't there.
    pub fn is_none(&self) -> bool {
        self.status.is_none() && self.queue_held.is_none() && self.initialized.is_none() && self.claims.is_none()
    }
}

impl From<StatusChange> for SessionChange {
    fn from(status: StatusChange) -> Self {
        Self { status: Some(status), ..Self::default() }
    }
}

impl From<SessionStatus> for SessionChange {
    fn from(status: SessionStatus) -> Self {
        StatusChange::from(status).into()
    }
}

/// Makes `change` to the session, as part of the transaction that records the event at `seq`, at `ts_ms`.
pub(super) fn change_session(
    tx: &Connection,
    session_id: &SessionId,
    seq: Seq,
    ts_ms: i64,
    change: SessionChange,
) -> Result<(), StoreError> {
    let id = session_id.as_str();
    if let Some(init) = change.initialized {
        tx.execute(
            "UPDATE sessions SET agent_opened = 1, agent_session_id = coalesce(?, agent_session_id) WHERE id = ?",
            params![init.conversation.as_ref().map(protocol::model::AgentSessionId::as_str), id],
        )?;
    }
    if let Some(claims) = change.claims {
        super::super::claims::set_claims(tx, session_id, &claims)?;
    }
    if let Some(held) = change.queue_held {
        tx.execute("UPDATE sessions SET queue_held = ? WHERE id = ?", params![held, id])?;
    }
    if let Some((run, through)) = change.recorded {
        tx.execute(
            "UPDATE sessions SET recorded_run = ?, recorded_through = ? WHERE id = ?",
            params![run.get().cast_signed(), through.cast_signed(), id],
        )?;
    }
    let Some(StatusChange { status, turn, background_tasks }) = change.status else { return Ok(()) };
    if let Some(tasks) = background_tasks {
        tx.execute(
            "UPDATE sessions SET background_tasks = ? WHERE id = ?",
            params![serde_json::to_string(&tasks)?, id],
        )?;
    }
    let status = text_of(&status);
    match turn {
        TurnEdge::Starts => tx.execute(
            "UPDATE sessions SET status = ?, turn_started_at_ms = ? WHERE id = ?",
            params![status, ts_ms, id],
        )?,
        TurnEdge::Ends(outcome) => tx.execute(
            "UPDATE sessions SET status = ?, reply_seq = ?, reply_outcome = ? WHERE id = ?",
            params![status, seq_column(seq), outcome.as_ref().map(text_of), id],
        )?,
        TurnEdge::Neither => tx.execute("UPDATE sessions SET status = ? WHERE id = ?", params![status, id])?,
    };
    Ok(())
}
