//! What a worker of protocol 4, the version before this one, sends that this one has no form for: what its agents say
//! of their context, their requests' prompt cache and their account's limits, each as a frame of its own, where this
//! protocol sends a [`Report`](crate::capability::Report). A worker of an older build speaks it until the hub replaces
//! it with one of its own, once its workspace is idle. Only the hub uses it, and it goes with the next break, as
//! `docs/principles/protocol.md` says.
//!
//! Each keeps protocol 4's wire form, which `crates/protocol/tests/compatibility/` spells out, so a change to a type
//! they're made of that changes that form fails there, rather than each being a frozen copy of those types.
//!
//! What the hub sends such a worker, and everything else such a worker sends, is this protocol's, so until protocol 4
//! goes, a change can't take away a form either reads or sends without keeping a way to read or send it.

use serde::Deserialize;

use crate::capability::Report;
use crate::capability::account::Limit;
use crate::model::{CacheRequest, ContextUsage, RunId, SessionId, UsageLimit};
use crate::worker::WorkerFrame;

/// Protocol 4, the version before [`crate::worker::WORKER_PROTOCOL_VERSION`].
pub const VERSION: u32 = 4;

/// A frame of protocol 4 that this protocol sends as a [`WorkerFrame::AgentReport`], which the hub reads as that. Each
/// `seq` is as that frame's is.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum Frame {
    /// How full the agent's context is now.
    AgentContext {
        session_id: SessionId,
        run: RunId,
        context: ContextUsage,
        #[serde(default)]
        seq: Option<u64>,
    },
    /// One of the agent's own requests to its model, as the model started to answer it.
    AgentCache {
        session_id: SessionId,
        run: RunId,
        request: CacheRequest,
        #[serde(default)]
        seq: Option<u64>,
    },
    /// What's used of each of the account's limits, as the agent answered when the worker asked.
    AgentLimits {
        session_id: SessionId,
        run: RunId,
        limits: Vec<UsageLimit>,
        /// Whether `limits` holds every model's own week the account has. `false` from a worker that doesn't say.
        #[serde(default)]
        every_models_week: bool,
        #[serde(default)]
        seq: Option<u64>,
    },
}

impl From<Frame> for WorkerFrame {
    fn from(frame: Frame) -> Self {
        let (session_id, run, report, seq) = match frame {
            Frame::AgentContext { session_id, run, context, seq } => {
                (session_id, run, Report::Context { context }, seq)
            }
            Frame::AgentCache { session_id, run, request, seq } => (session_id, run, Report::Cache { request }, seq),
            Frame::AgentLimits { session_id, run, limits, every_models_week, seq } => {
                let limits = limits.iter().map(Limit::of).collect();
                (session_id, run, Report::Limits { limits, every_models_week }, seq)
            }
        };
        WorkerFrame::AgentReport { session_id, run, report, seq }
    }
}
