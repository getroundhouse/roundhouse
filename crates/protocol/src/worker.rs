//! What a hub says to a worker, over whatever byte stream the provider returns.
//!
//! Workers are told what to do and report what happened. They don't own any lasting records: the
//! hub assigns sequence numbers and stores events.

use serde::{Deserialize, Serialize};

use crate::attachment::{AttachmentBytes, AttachmentFile, NewAttachment};
use crate::client::ApiError;
use crate::event::{ApprovalDecision, EventBody};
use crate::model::{
    AgentAccount, AgentKind, AgentSessionId, ApprovalId, CacheRequest, CommitId, ContextUsage, FileContents, GitDiff,
    GitStatus, PullRequest, RunId, Seq, SessionId, SessionSettings, SessionSettingsChange, UsageLimit, WorktreeFiles,
};
use crate::tail::StreamDelta;

/// The worker protocol's version, apart from the client protocol's, so a break in one doesn't break the other. It
/// changes only for a real break, as "Changing the protocol" in `docs/architecture.md` says.
pub const WORKER_PROTOCOL_VERSION: u32 = 3;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkerCall {
    pub id: u64,
    pub request: WorkerRequest,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum WorkerRequest {
    /// It names the newest version of the protocol the hub speaks, and the oldest, which a hub from before ranges
    /// leaves out, since it speaks only the one.
    Hello {
        protocol_version: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        oldest_protocol_version: Option<u32>,
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
    /// The files in the worktree at `path`, as [`crate::client::ClientRequest::WorkspaceFiles`] says.
    ListFiles {
        path: String,
    },
    /// What's directly in the folder at `folder`, relative to the worktree at `path`. Refused unless it's a folder
    /// inside the worktree once every link is followed.
    FolderRead {
        path: String,
        folder: String,
    },
    /// What the file at `file` holds, relative to the worktree at `path`. Refused unless it's a file inside the
    /// worktree once every link is followed.
    FileRead {
        path: String,
        file: String,
    },
    /// What the file at `file` held in `commit`, relative to the worktree at `path`, from the worktree's repository.
    FileReadAt {
        path: String,
        commit: CommitId,
        file: String,
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
    /// Asks the agent to compact its conversation. `asked` is where that's recorded, which the agent's
    /// [`crate::event::EventBody::PromptTaken`] and its compaction name.
    AgentCompact {
        session_id: SessionId,
        asked: Seq,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keeping: Option<String>,
    },
    /// A setting left out is left as it is.
    AgentConfigure {
        session_id: SessionId,
        #[serde(flatten)]
        options: SessionSettingsChange,
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
        /// The prompt's images, read with its words, since a prompt such as "how do we fix this?" can be about a
        /// screenshot. Any that isn't an image, such as a kind from a newer hub, is left out. A worker from before
        /// images ignores them, and titles from the words alone.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<NewAttachment>,
    },
    /// The account `agent`'s CLI is logged in to on this machine, from a call of its own to the CLI, outside any
    /// session. It changes nothing. Every agent of a kind here runs under one login today. When they can run under
    /// several, this says which one's it asks for.
    AccountFor {
        agent: AgentKind,
    },
    /// Any request this build doesn't know, from a hub newer than it. A worker answers with
    /// [`ErrorCode::Unsupported`](crate::client::ErrorCode::Unsupported) and carries on, as a hub does a client's.
    #[serde(other)]
    Unknown,
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
    /// What it runs with.
    #[serde(flatten)]
    pub settings: SessionSettings,
    /// Set in the agent's environment, along with the worker's own.
    pub env: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum WorkerResponse {
    Hello {
        /// The one chosen: the newest both sides speak.
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
    Files(WorktreeFiles),
    FileContents(FileContents),
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
    Account(AgentAccount),
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
    /// How full the agent's context is now. It isn't an event: it answers the worker's own question rather than
    /// being something the agent said, and only the newest matters, which the hub keeps on the session's record.
    AgentContext {
        session_id: SessionId,
        run: RunId,
        context: ContextUsage,
    },
    /// One of the agent's own requests to its model, as the model started to answer it. It isn't an event, since a
    /// turn makes many, but each matters: the hub keeps what the newest left on the session's record, and compares
    /// each with the one before to find a cache that had expired.
    AgentCache {
        session_id: SessionId,
        run: RunId,
        request: CacheRequest,
    },
    /// What's used of each of the account's limits, as the agent answered when the worker asked. It isn't an event,
    /// as how full the context is isn't: the agent wasn't told it as it worked, and the hub keeps the newest on the
    /// account's record, as it does what the agent hears by itself.
    AgentLimits {
        session_id: SessionId,
        run: RunId,
        limits: Vec<UsageLimit>,
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
