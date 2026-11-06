//! What a hub says to a worker, over whatever byte stream the provider returns.
//!
//! Workers are told what to do and report what happened. They don't own any lasting records: the
//! hub assigns sequence numbers and stores events.

use serde::{Deserialize, Serialize};

use crate::client::ApiError;
use crate::event::{ApprovalDecision, EventBody};
use crate::model::{AgentKind, GitDiff, GitStatus, SessionId};
use crate::tail::StreamDelta;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkerCall {
    pub id: u64,
    pub request: WorkerRequest,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum WorkerRequest {
    Hello {
        protocol_version: u32,
    },
    /// Creates the worktree and its branch. Fails with `Conflict` if either already exists.
    PrepareWorkspace {
        repo_name: String,
        repo_path: String,
        workspace_name: String,
        branch: String,
        base_ref: Option<String>,
    },
    /// Puts the worktree back from its kept branch.
    RestoreWorkspace {
        repo_path: String,
        path: String,
        branch: String,
    },
    RemoveWorkspace {
        repo_path: String,
        path: String,
        force: bool,
    },
    GitStatus {
        path: String,
    },
    GitDiff {
        path: String,
        base_ref: Option<String>,
    },
    AgentStart(AgentSpec),
    AgentPrompt {
        session_id: SessionId,
        text: String,
    },
    AgentApproval {
        session_id: SessionId,
        approval_id: String,
        decision: ApprovalDecision,
    },
    AgentCancel {
        session_id: SessionId,
    },
    /// Resolves once the process has exited.
    AgentStop {
        session_id: SessionId,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentSpec {
    pub session_id: SessionId,
    /// Chosen by the hub and unique to this launch. The agent's events carry it, so a late event
    /// from an agent that has been replaced can't be taken for one from the new agent.
    pub run: u64,
    pub agent: AgentKind,
    pub cwd: String,
    /// The agent's own ID for this conversation, which survives restarts.
    pub agent_session_id: String,
    /// Continue that conversation rather than starting it.
    pub resume: bool,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum WorkerResponse {
    Hello {
        protocol_version: u32,
    },
    Ok,
    WorkspaceReady {
        path: String,
        branch: String,
        base_ref: String,
        #[serde(default)]
        warnings: Vec<String>,
    },
    GitStatus(GitStatus),
    GitDiff(GitDiff),
    /// `run` is the one already going if `already_running` is set.
    AgentStarted {
        pid: u32,
        run: u64,
        already_running: bool,
    },
    /// The session has no running agent. The hub starts one and sends the request again.
    AgentNotRunning,
}

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
        run: u64,
        body: EventBody,
        /// The agent's original line, kept by the hub beside the event.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw: Option<String>,
    },
    AgentDelta {
        session_id: SessionId,
        delta: StreamDelta,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerOutcome {
    Response(WorkerResponse),
    Error(ApiError),
}
