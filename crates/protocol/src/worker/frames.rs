//! What a worker sends its hub: the reply to each call, and, as it happens, what its agents say, what it asks, and the
//! branches it watches.

use serde::{Deserialize, Serialize};

use super::{AskId, WorkerAsk, WorkerOutcome};
use crate::attachment::NewAttachment;
use crate::event::EventBody;
use crate::model::{Branches, CacheRequest, ContextUsage, RunId, SessionId, UsageLimit};
use crate::tail::StreamDelta;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum WorkerFrame {
    Reply {
        id: u64,
        #[serde(flatten)]
        outcome: WorkerOutcome,
    },
    AgentEvent {
        session_id: SessionId,
        run: RunId,
        body: EventBody,
        /// The agent's original line, kept by the hub beside the event, without the bytes of any of `attachments`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw: Option<String>,
        /// Images the event names, such as one a tool sent back, for the hub to keep before it records the event.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<NewAttachment>,
        /// Its number among what the worker keeps until a hub has recorded it, for a hub that said
        /// [`keep_running_without_hub`](super::WorkerRequest::Hello::keep_running_without_hub). `None` otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
    AgentDelta {
        session_id: SessionId,
        run: RunId,
        delta: StreamDelta,
    },
    /// How full the agent's context is now. It isn't an event: it answers the worker's own question rather than
    /// being something the agent said, and only the newest matters, which the hub keeps on the session's record.
    AgentContext {
        session_id: SessionId,
        run: RunId,
        context: ContextUsage,
        /// Its number among what the worker keeps until a hub has recorded it, for a hub that said
        /// [`keep_running_without_hub`](super::WorkerRequest::Hello::keep_running_without_hub). `None` otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
    /// One of the agent's own requests to its model, as the model started to answer it. It isn't an event, since a
    /// turn makes many, but each matters: the hub keeps what the newest left on the session's record, and compares
    /// each with the one before to find a cache that had expired.
    AgentCache {
        session_id: SessionId,
        run: RunId,
        request: CacheRequest,
        /// Its number among what the worker keeps until a hub has recorded it, for a hub that said
        /// [`keep_running_without_hub`](super::WorkerRequest::Hello::keep_running_without_hub). `None` otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
    /// What's used of each of the account's limits, as the agent answered when the worker asked. It isn't an event,
    /// as how full the context is isn't: the agent wasn't told it as it worked, and the hub keeps the newest on the
    /// account's record, as it does what the agent hears by itself.
    AgentLimits {
        session_id: SessionId,
        run: RunId,
        limits: Vec<UsageLimit>,
        /// Its number among what the worker keeps until a hub has recorded it, for a hub that said
        /// [`keep_running_without_hub`](super::WorkerRequest::Hello::keep_running_without_hub). `None` otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
    /// Something the agent in `session_id` called one of the worker's tools for, which only the hub can answer. The
    /// hub answers with an [`AskAnswer`](super::AskAnswer) naming `id`, which counts apart from the hub's calls.
    Ask {
        id: AskId,
        session_id: SessionId,
        ask: WorkerAsk,
    },
    /// What the watched worktree's branches are now, whole, as
    /// [`WatchBranches`](super::WorkerRequest::WatchBranches) asked, so a frame replaces every one before it, and only
    /// the newest matters.
    Branches {
        branches: Branches,
    },
}
