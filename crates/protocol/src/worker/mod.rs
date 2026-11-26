//! What a hub says to a worker, over whatever byte stream the provider returns.
//!
//! Workers are told what to do and report what happened. They don't own any lasting records: the
//! hub assigns sequence numbers and stores events.
//!
//! Each request is a type of its own, which says what the worker answers it with, as [`Request`] does, so a hub reads
//! each reply as that, and one of any other kind fails in one place. On the wire, a request's fields sit beside its
//! `method`, and a reply's beside its `result`.

mod agents;
mod archived;
mod ask;
mod children;
mod connection;
mod frames;
mod git_credential;
mod one_off;
pub mod previous;
mod repo_source;
mod repo_url;
mod terminals;
mod tool_servers;
mod tools;
mod workspaces;
mod worktree;

pub use agents::{AgentControl, AgentSpec, AgentStarted, AgentStop, Delivery};
pub use archived::{ArchivedBranch, ArchivedBundle, ArchivedGit, CaptureArchive, MAX_ARCHIVED_BUNDLE_BYTES};
pub use ask::{
    Answer, Ask, AskAnswer, AskId, AskOutcome, AskReply, CancelWait, CreateWorktree, DiffComments, ListWorktrees,
    ListedChild, NewWorktreeOn, OwnWorktree, ProposalRecord, Propose, PullRequestAnswer, PullRequestWaitUntil,
    ReadDiffComments, ReadProposal, ReadPullRequest, ReadPullRequests, ReadWait, RemoveWorktree, RenewToolServer,
    Revise, StartPullRequestWait, ToWorker, WaitAnswer, Withdraw, WorkerAsk, WorktreeCreated, WorktreeRemoval,
    WorktreeRemoved, Worktrees, WouldLose,
};
pub use children::{AddChildWorktree, ChildBranch, ChildWorktreeAdded, WatchChildBranches};
pub use connection::{Hello, HelloReply, Received, RunningAgent, RunningAgents, RunningAgentsReply, Stop};
pub use frames::WorkerFrame;
pub use git_credential::{GitCredential, GitCredentialSet, GitIdentity, GithubCredential};
pub use one_off::{AccountFor, TitleFor, TitleReply};
pub use repo_source::RepoSource;
pub use repo_url::{ReadRepoUrl, RepoUrlReply};
pub use terminals::{
    RunningTerminal, RunningTerminals, RunningTerminalsReply, TerminalBounds, TerminalFit, TerminalRows, TerminalStart,
    TerminalStop, TerminalText, TerminalTyped, TerminalWatch,
};
pub use tool_servers::{ProxiedServer, ServerHeader, ServerHeaderValue, ServerSignIn, TokenNumber, ToolServersSet};
pub use tools::{ServeTools, TOOLS_SERVER, ToolsServing, ToolsToken, WAIT_TOOL, WAITS_TOOL};
pub use workspaces::{
    ArchiveScript, CancelSetup, NamesGitHas, NamesGitHasReply, PrepareWorkspace, RemoveWorkspace, RestoreWorkspace,
    SetupDone, SetupLog, SetupLogReply, SetupWorkspace, WorkspaceReady, WorktreeThere, WorktreeThereReply,
};
pub use worktree::{
    AttachmentWrite, AttachmentWritten, DiffFile, FileOpenAt, FilePages, FileRead, FileReadAt, FileReadRange,
    FolderRead, GitDiff, GitStatus, ListChangedFiles, ListFiles, SummarizeChanges, WatchBranches, WatchChanges,
};

mod messages;
pub use messages::{
    HUB_SPEAKS, Reply, Request, WORKER_PROTOCOL_VERSION, WorkerCall, WorkerOutcome, WorkerRequest, WorkerResponse,
};
