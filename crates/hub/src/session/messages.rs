//! What a session is sent: what arrives from its agent's worker, and what clients ask of it.

use protocol::attachment::NewAttachment;
use protocol::capability::Report;
use protocol::capability::approvals::Decision;
use protocol::client::ApiError;
use protocol::event::{EventBody, ExitReason};
use protocol::model::{ApprovalId, AttachmentId, ClientDevice, ClientMessageId, RunId, Seq, Session};
use protocol::prompt::Prompt;
use protocol::tail::{Retrying, StreamDelta, Tail};
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
    /// The agent of `run` is trying a failed request to its model again, which goes in the tail until it writes again.
    Retrying {
        run: RunId,
        retrying: Retrying,
    },
    /// What the agent of `run` is writing, as its worker kept it for this hub, which missed the deltas that wrote it. It
    /// comes after what the worker kept for the hub and before anything new, so the deltas after it follow on from it.
    Tail {
        run: RunId,
        tail: Tail,
    },
    /// What the agent of `run` reported that isn't an event, which the session keeps where the report's keeper says. It
    /// comes in the queue with the agent's events, so it's never taken for a later run's, or kept after the run has
    /// gone, and so that a request to the model that found the cache expired is recorded before what the agent wrote.
    Report {
        run: RunId,
        report: Report,
        /// Its number among what the agent's worker sent, for a hub that asked the worker to keep running without it.
        seq: Option<u64>,
        /// Whether it's dropped unless its run claims what it needs, as it is but from a worker of protocol 4, which
        /// sent each of its reports as a frame of its own, from before a run had to claim what a report needs.
        held_to_claims: bool,
    },
    /// The agent on the worker connection `link` has gone, unless a newer one has replaced it, as when the connection
    /// ends. Everything it sent has been queued ahead of this.
    AgentLost {
        link: u64,
        reason: ExitReason,
    },
    /// The agent has had long enough to pick up again after its work in the background, as [`super::pickup`] says, in
    /// this `wait`. In the queue with the agent's events, which may have ended the wait.
    PickupDue {
        wait: u64,
    },
    /// Resolves once everything queued ahead of it has been recorded.
    Flush(oneshot::Sender<()>),
    /// The session is gone. What's queued ahead of this is dealt with, and then the task stops.
    End,
}

pub(crate) enum Command {
    Prompt {
        client_msg_id: Option<ClientMessageId>,
        prompt: Prompt<AttachmentId>,
        /// The device the person sent it from, if their client said one in its hello.
        device: Option<ClientDevice>,
        reply: oneshot::Sender<Result<(Seq, bool), ApiError>>,
    },
    Decide {
        approval_id: ApprovalId,
        decision: Decision,
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
    /// Starts the agent, if it's waiting on nothing but a wait, so it hears how the wait ended.
    WakeAgent,
    /// Sends the prompts kept for the turn's end, which has come.
    SendAfterTheTurn,
    /// The session is gone. What's queued ahead of this is carried out, and then the task stops.
    End,
}
