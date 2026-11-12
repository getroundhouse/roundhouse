//! What a hub says to a worker, over whatever byte stream the provider returns.
//!
//! Workers are told what to do and report what happened. They don't own any lasting records: the
//! hub assigns sequence numbers and stores events.

use serde::{Deserialize, Serialize};

use crate::attachment::{AttachmentBytes, AttachmentFile, NewAttachment};
use crate::client::Outcome;
use crate::event::{ApprovalDecision, EventBody};
use crate::model::{
    AgentAccount, AgentKind, AgentSessionId, ApprovalId, BranchName, Branches, CacheRequest, CommitId, ContextUsage,
    FileContents, FileOpened, FilePages, GitDiff, GitStatus, PullRequest, RunId, Seq, SessionId, SessionSettings,
    SessionSettingsChange, UsageLimit, WorktreeFiles,
};
use crate::tail::StreamDelta;

/// The worker protocol's version, apart from the client protocol's, so a break in one doesn't break the other. It
/// changes only for a real break, as "Changing the protocol" in `docs/architecture.md` says.
pub const WORKER_PROTOCOL_VERSION: u32 = 3;

mod ask;
mod tools;

pub use ask::{AskAnswer, AskId, AskOutcome, AskReply, ToWorker, WorkerAsk};
pub use tools::ToolsToken;

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
    /// Watches the branches of the worktree at `path`, and sends a [`WorkerFrame::Branches`] with what they are at
    /// once, and again each time they change, until the worker stops. A later watch replaces it. Replies `Ok` once
    /// it's watching and has sent that first frame, or found it can't read them, so a hub that waits for the reply
    /// has the frame first.
    WatchBranches {
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
    /// Opens the file at `file` as `commit` held it, relative to the worktree at `path`, from the worktree's
    /// repository, to be read a page at a time.
    FileOpenAt {
        path: String,
        commit: CommitId,
        file: String,
    },
    /// Pages of a file opened with `FileOpenAt`, with `page_lines` lines a page.
    FilePages {
        path: String,
        commit: CommitId,
        file: String,
        page_lines: u32,
        pages: Vec<u32>,
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
    /// Serves the worker's tools to its agents, over MCP on 127.0.0.1, on `port` if it's free and on a port of the
    /// system's choosing if not, or if there's none. Replies with the port it's on, and harmless once it's serving,
    /// when it replies with the port it's already on. Each agent reaches it with the token its
    /// [`AgentSpec::tools_token`] names.
    ServeTools {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port: Option<u16>,
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
    /// What the agent's calls to the worker's tools carry, which tells the worker whose they are. The hub keeps one for
    /// each session, so every launch of its agent has the same. `None` when the worker isn't serving its tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools_token: Option<ToolsToken>,
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
        /// The worktree's branches, as it was made, which a watch then starts from. `None` when they couldn't be read.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branches: Option<Branches>,
    },
    GitStatus(GitStatus),
    GitDiff(GitDiff),
    PullRequest {
        pull_request: Option<PullRequest>,
        /// The branch checked out as it looked, whose pull request this is. `None` on a detached HEAD, and from a
        /// worker that doesn't say.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchName>,
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
    FileOpened(FileOpened),
    FilePages(FilePages),
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
    /// The port the worker's tools are served on, on 127.0.0.1.
    ToolsServing {
        port: u16,
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
    /// Something the agent in `session_id` called one of the worker's tools for, which only the hub can answer. The
    /// hub answers with an [`AskAnswer`] naming `id`, which counts apart from the hub's calls.
    Ask {
        id: AskId,
        session_id: SessionId,
        ask: WorkerAsk,
    },
    /// What the watched worktree's branches are now, whole, as [`WorkerRequest::WatchBranches`] asked, so a frame
    /// replaces every one before it, and only the newest matters.
    Branches {
        branches: Branches,
    },
}

pub type WorkerOutcome = Outcome<WorkerResponse>;

/// GitHub's hourly limit on the GraphQL calls `gh pr view` makes, as GitHub counted them when asked.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateLimit {
    pub limit: u32,
    pub remaining: u32,
    /// When it's back to `limit`.
    pub resets_at_ms: i64,
}
