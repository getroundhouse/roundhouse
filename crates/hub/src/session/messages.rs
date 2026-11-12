//! What a session is sent: what arrives from its agent's worker, and what clients ask of it.

use protocol::attachment::NewAttachment;
use protocol::client::ApiError;
use protocol::event::{ApprovalDecision, EventBody, ExitReason};
use protocol::model::{
    ApprovalId, AttachmentId, CacheRequest, ClientMessageId, ContextUsage, RunId, Seq, Session, UsageLimit,
};
use protocol::tail::StreamDelta;
use tokio::sync::oneshot;

/// What arrives from a worker.
pub(crate) enum Item {
    Event {
        run: RunId,
        /// Boxed, since it's far larger than anything else a session is sent.
        body: Box<EventBody>,
        raw: Option<String>,
        /// Images `body` names, which are kept before it's recorded.
        attachments: Vec<NewAttachment>,
        /// Its number among what the agent's worker sent, for a hub that asked the worker to keep running without it.
        seq: Option<u64>,
    },
    Delta {
        run: RunId,
        delta: StreamDelta,
    },
    /// How full the agent's context is now. It comes in the queue with the agent's events, so it's never taken for
    /// a later run's, or kept after the run has gone.
    Context {
        run: RunId,
        context: ContextUsage,
        /// Its number among what the agent's worker sent, for a hub that asked the worker to keep running without it.
        seq: Option<u64>,
    },
    /// One of the agent's own requests to its model, as it began. It comes in the queue with the agent's events for
    /// the same reason, and so that a request that found the cache expired is recorded before what the agent wrote.
    Cache {
        run: RunId,
        request: CacheRequest,
        /// Its number among what the agent's worker sent, for a hub that asked the worker to keep running without it.
        seq: Option<u64>,
    },
    /// What's used of the account's limits, as the agent answered when its worker asked. It comes in the queue with
    /// the agent's events for the same reason.
    Limits {
        run: RunId,
        limits: Vec<UsageLimit>,
        /// Its number among what the agent's worker sent, for a hub that asked the worker to keep running without it.
        seq: Option<u64>,
    },
    /// The agent on the worker connection `link` has gone, unless a newer one has replaced it, as when the connection
    /// ends. Everything it sent has been queued ahead of this.
    AgentLost {
        link: u64,
        reason: ExitReason,
    },
    /// Resolves once everything queued ahead of it has been recorded.
    Flush(oneshot::Sender<()>),
    /// The session is gone. What's queued ahead of this is dealt with, and then the task stops.
    End,
}

pub(crate) enum Command {
    Prompt {
        text: String,
        client_msg_id: Option<ClientMessageId>,
        attachments: Vec<AttachmentId>,
        reply: oneshot::Sender<Result<(Seq, bool), ApiError>>,
    },
    Approve {
        approval_id: ApprovalId,
        decision: ApprovalDecision,
        reply: oneshot::Sender<Result<(), ApiError>>,
    },
    Cancel {
        reply: oneshot::Sender<Result<(), ApiError>>,
    },
    Compact {
        keeping: Option<String>,
        reply: oneshot::Sender<Result<(), ApiError>>,
    },
    Stop {
        reply: oneshot::Sender<Result<(), ApiError>>,
    },
    /// Sends the first queued prompt, if the agent is ready for it by the time this runs.
    SendQueued,
    /// Sends the queued prompt with this ID now, whether or not the agent is ready for it.
    SendQueuedNow {
        client_msg_id: ClientMessageId,
        reply: oneshot::Sender<Result<Session, ApiError>>,
    },
    /// The session is gone. What's queued ahead of this is carried out, and then the task stops.
    End,
}
