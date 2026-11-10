//! What a hub says to a worker, over whatever byte stream the provider returns.
//!
//! Workers are told what to do and report what happened. They don't own any lasting records: the
//! hub assigns sequence numbers and stores events.

use serde::{Deserialize, Serialize};

use crate::attachment::{AttachmentBytes, AttachmentFile, MediaType, NewAttachment};
use crate::client::ApiError;
use crate::event::{ApprovalDecision, EventBody};
use crate::model::{
    AgentKind, AgentSessionId, ApprovalId, GitDiff, GitStatus, PullRequest, RunId, Seq, SessionId, SessionOptions,
};
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
    /// Creates the worktree and its branch. Fails with `NameTaken` if either already exists.
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
    PullRequest {
        path: String,
    },
    /// How much of GitHub's hourly limit is left for the `gh` that runs in this worktree. Asking costs none of it.
    GithubRateLimit {
        path: String,
    },
    /// Copies local files and runs the setup script. Resolves when it's finished, however long that is.
    SetupWorkspace {
        repo_path: String,
        path: String,
        env: Vec<(String, String)>,
    },
    /// Stops the setup script, including one asked for just before this that hasn't started yet. Harmless if
    /// there isn't one.
    CancelSetup,
    SetupLog {
        path: String,
    },
    /// Runs the archive script, if there is one. Fails if the script does.
    ArchiveScript {
        repo_path: String,
        path: String,
        env: Vec<(String, String)>,
    },
    /// Writes a file a prompt carries into the worktree at `path`, in `.context/attachments/`, which git ignores, and
    /// replies with where it went. The same bytes under the same name find the copy already there, so sending it
    /// again makes no second one, and a different file with the same name gets a name of its own.
    AttachmentWrite {
        path: String,
        name: String,
        data: AttachmentBytes,
    },
    /// The image at `image` in the worktree at `path`, where `image` is in full or relative to the worktree, as the
    /// agent wrote it. Refused unless it's in the worktree once every link is followed, and is an image the transcript
    /// draws, since what an agent writes can point anywhere.
    ImageRead {
        path: String,
        image: String,
    },
    AgentStart(AgentSpec),
    /// Each of `attachments` has been written into the worktree first. `prompt` is where it's recorded, which the
    /// agent's [`crate::event::EventBody::PromptTaken`] names.
    AgentPrompt {
        session_id: SessionId,
        prompt: Seq,
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<AttachmentFile>,
    },
    AgentApproval {
        session_id: SessionId,
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    },
    AgentCancel {
        session_id: SessionId,
    },
    /// A setting left out is left as it is.
    AgentConfigure {
        session_id: SessionId,
        #[serde(flatten)]
        options: SessionOptions,
    },
    /// Resolves once the process has exited.
    AgentStop {
        session_id: SessionId,
    },
    /// A short title for a prompt, from a call of its own to a small model of `agent`'s, outside any session. It
    /// changes nothing, so sending it again only asks again.
    TitleFor {
        agent: AgentKind,
        prompt: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentSpec {
    pub session_id: SessionId,
    /// Chosen by the hub and unique to this launch. The agent's output carries it, so a late event
    /// or delta from an agent that has been replaced can't be taken for one from the new agent.
    pub run: RunId,
    pub agent: AgentKind,
    pub cwd: String,
    /// The agent's own ID for this conversation, which survives restarts.
    pub agent_session_id: AgentSessionId,
    /// Continue that conversation rather than starting it.
    pub resume: bool,
    /// What it runs with. A setting left out is left to the user's own agent settings.
    #[serde(flatten)]
    pub options: SessionOptions,
    /// Set in the agent's environment, along with the worker's own.
    pub env: Vec<(String, String)>,
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
    PullRequest {
        pull_request: Option<PullRequest>,
    },
    GithubRateLimit(RateLimit),
    SetupDone {
        /// `None` if it was stopped.
        code: Option<i32>,
    },
    SetupLog {
        text: String,
    },
    /// Where an attachment was written, in full.
    AttachmentWritten {
        path: String,
    },
    /// An image, and what kind it is by its bytes.
    Image {
        media_type: MediaType,
        data: AttachmentBytes,
    },
    /// `run` is the one already going if `already_running` is set.
    AgentStarted {
        pid: u32,
        run: RunId,
        already_running: bool,
    },
    /// The session has no running agent. The hub starts one and sends the request again.
    AgentNotRunning,
    Title {
        title: String,
    },
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
        run: RunId,
        body: EventBody,
        /// The agent's original line, kept by the hub beside the event, without the bytes of any of `attachments`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw: Option<String>,
        /// Images the event names, such as one a tool sent back, for the hub to keep before it records the event.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<NewAttachment>,
    },
    AgentDelta {
        session_id: SessionId,
        run: RunId,
        delta: StreamDelta,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerOutcome {
    Response(WorkerResponse),
    Error(ApiError),
}

/// GitHub's hourly limit on the GraphQL calls `gh pr view` makes, as GitHub counted them when asked.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateLimit {
    pub limit: u32,
    pub remaining: u32,
    /// When it's back to `limit`.
    pub resets_at_ms: i64,
}
