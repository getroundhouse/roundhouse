//! What a worker sends its hub: the reply to each call, and, as it happens, what its agents say, what it asks, the
//! branches it watches, and what its terminals' programs write.

use serde::{Deserialize, Serialize};

use super::{AskId, WorkerAsk, WorkerOutcome};
use crate::attachment::NewAttachment;
use crate::capability::Report;
use crate::event::EventBody;
use crate::model::{Branches, RunId, SessionId, TerminalId};
use crate::tail::{Retrying, StreamDelta, Tail};
use crate::terminal::{TerminalUpdate, UpdateIndex};

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
        /// [`keep_running_without_hub`](super::Hello::keep_running_without_hub). `None` otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
    AgentDelta {
        session_id: SessionId,
        run: RunId,
        delta: StreamDelta,
    },
    /// The agent of `run` is trying a failed request to its model again. It isn't an event: only the newest matters,
    /// and only until the agent writes again, so the hub keeps it in the session's tail, as a delta is.
    AgentRetrying {
        session_id: SessionId,
        run: RunId,
        retrying: Retrying,
    },
    /// What the agent of `run` is writing, as the deltas the worker has sent make it, with the subagents it runs in the
    /// background and its compaction under way. Sent to a hub as it says hello, after what the worker kept for it and
    /// before anything new, since the deltas that wrote it went to the hub before, or to none, so this hub's tail of
    /// `run` is this from here on. Only for a run with something in it. *Unconfirmed.*
    AgentTail {
        session_id: SessionId,
        run: RunId,
        tail: Tail,
    },
    /// What the agent of `run` said that isn't an event, which the hub keeps where the report's keeper says, if the run
    /// claims what it needs, as [`Report`] says, and drops otherwise.
    AgentReport {
        session_id: SessionId,
        run: RunId,
        report: Report,
        /// Its number among what the worker keeps until a hub has recorded it, for a hub that said
        /// [`keep_running_without_hub`](super::Hello::keep_running_without_hub). `None` otherwise.
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
    /// [`WatchBranches`](super::WatchBranches) asked, so a frame replaces every one before it, and only
    /// the newest matters.
    Branches {
        branches: Branches,
    },
    /// What the branches of another of the workspace's worktrees are now, whole, as
    /// [`WatchChildBranches`](super::WatchChildBranches) asked, so a frame replaces every one before it
    /// for that worktree.
    ChildBranches {
        path: String,
        branches: Branches,
    },
    /// What happens next in a terminal: what its program wrote, a new size, or its end, but never a screen, which
    /// comes only in a [`TerminalShown`](super::TerminalShown). It isn't kept for a hub that isn't there, since a hub
    /// that connects asks what the terminal shows, and that takes it in.
    Terminal {
        terminal: TerminalId,
        /// So a hub can tell which updates come after a screen it asked for.
        at: UpdateIndex,
        update: TerminalUpdate,
    },
}
