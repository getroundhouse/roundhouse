//! What a hub says to a worker, over whatever byte stream the provider returns.
//!
//! Workers are told what to do and report what happened. They don't own any lasting records: the
//! hub assigns sequence numbers and stores events.
//!
//! Each request is a type of its own, which says what the worker answers it with, as [`Request`] does, so a hub reads
//! each reply as that, and one of any other kind fails in one place. On the wire, a request's fields sit beside its
//! `method`, and a reply's beside its `result`.

mod agents;
mod ask;
mod children;
mod connection;
mod frames;
mod github;
mod one_off;
pub mod previous;
mod tools;
mod workspaces;
mod worktree;

pub use agents::{AgentApproval, AgentControl, AgentSpec, AgentStarted, AgentStop, Delivery};
pub use ask::{
    Answer, Ask, AskAnswer, AskId, AskOutcome, AskReply, CancelWait, CreateWorktree, ListWorktrees, ListedChild,
    NewWorktreeOn, OwnWorktree, PullRequestAnswer, PullRequestWaitUntil, ReadPullRequest, ReadPullRequests, ReadWait,
    RemoveWorktree, StartPullRequestWait, StartWait, ToWorker, WaitAnswer, WorkerAsk, WorktreeCreated, WorktreeRemoval,
    WorktreeRemoved, Worktrees, WouldLose,
};
pub use children::{AddChildWorktree, ChildBranch, ChildWorktreeAdded, WatchChildBranches};
pub use connection::{Hello, HelloReply, Received, RunningAgent, RunningAgents, RunningAgentsReply, Stop};
pub use frames::WorkerFrame;
pub use github::{
    GithubRateLimit, PullRequest, PullRequestReply, RateLimit, ReadRemotePullRequests, RemotePullRequestsReply,
};
pub use one_off::{AccountFor, TitleFor, TitleReply};
pub use tools::{ServeTools, ToolsServing, ToolsToken, WAIT_TOOL, WAITS_TOOL};
pub use workspaces::{
    ArchiveScript, CancelSetup, NamesGitHas, NamesGitHasReply, PrepareWorkspace, RemoveWorkspace, RestoreWorkspace,
    SetupDone, SetupLog, SetupLogReply, SetupWorkspace, WorkspaceReady, WorktreeThere, WorktreeThereReply,
};
pub use worktree::{
    AttachmentWrite, AttachmentWritten, FileOpenAt, FilePages, FileRead, FileReadAt, FolderRead, GitDiff, GitStatus,
    ListFiles, WatchBranches,
};

mod messages;
pub use messages::{Reply, Request, WORKER_PROTOCOL_VERSION, WorkerCall, WorkerOutcome, WorkerRequest, WorkerResponse};
