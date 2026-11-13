//! What a hub says to a worker, over whatever byte stream the provider returns.
//!
//! Workers are told what to do and report what happened. They don't own any lasting records: the
//! hub assigns sequence numbers and stores events.
//!
//! Each request is a type of its own, which says what the worker answers it with, as [`Request`] does, so a hub reads
//! each reply as that, and one of any other kind fails in one place. On the wire, a request's fields sit beside its
//! `method`, and a reply's beside its `result`.

use serde::{Deserialize, Serialize};

use crate::client::Outcome;
use crate::model::{self, AgentAccount, FileContents, FileOpened, WorktreeFiles};

/// The worker protocol's version, apart from the client protocol's, so a break in one doesn't break the other. It
/// changes only for a real break, as "Changing the protocol" in `docs/architecture.md` says.
pub const WORKER_PROTOCOL_VERSION: u32 = 3;

mod agents;
mod ask;
mod connection;
mod frames;
mod github;
mod one_off;
mod tools;
mod workspaces;
mod worktree;

pub use agents::{
    AgentApproval, AgentCancel, AgentCompact, AgentConfigure, AgentPrompt, AgentSpec, AgentStarted, AgentStop, Delivery,
};
pub use ask::{AskAnswer, AskId, AskOutcome, AskReply, ToWorker, WorkerAsk};
pub use connection::{Hello, HelloReply, Received, RunningAgent, RunningAgents, RunningAgentsReply, Stop};
pub use frames::WorkerFrame;
pub use github::{GithubRateLimit, PullRequest, PullRequestReply, RateLimit};
pub use one_off::{AccountFor, TitleFor, TitleReply};
pub use tools::{ServeTools, ToolsServing, ToolsToken, WAIT_TOOL};
pub use workspaces::{
    ArchiveScript, CancelSetup, PrepareWorkspace, RemoveWorkspace, RestoreWorkspace, SetupDone, SetupLog,
    SetupLogReply, SetupWorkspace, WorkspaceReady, WorktreeThere, WorktreeThereReply,
};
pub use worktree::{
    AttachmentWrite, AttachmentWritten, FileOpenAt, FilePages, FileRead, FileReadAt, FolderRead, GitDiff, GitStatus,
    ListFiles, WatchBranches,
};

/// A request to a worker, which names what the worker answers it with.
pub trait Request {
    type Reply: Reply;

    /// It among every other request, as it goes on the wire.
    fn into_request(self) -> WorkerRequest;
}

/// What a worker answers a request with, read from the response it came as.
pub trait Reply: Sized {
    /// Gives the response back when it's of another kind, for whoever asked to say so.
    #[expect(
        clippy::result_large_err,
        reason = "a reply is moved once, as it's read, so boxing one of another kind would only add an allocation"
    )]
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse>;
}

/// What a request that only does something is answered with, once it's done.
impl Reply for () {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::Ok = response else { return Err(response) };
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkerCall {
    pub id: u64,
    pub request: WorkerRequest,
}

/// Every request. Each says on its type what it does and what it's answered with. One with no fields is a variant with
/// none, rather than one holding its type, since serde would then refuse it with a field a newer hub added, and its
/// type is only for saying what it's answered with.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum WorkerRequest {
    Hello(Hello),
    /// See [`Stop`].
    Stop,
    /// See [`RunningAgents`].
    RunningAgents,
    Received(Received),
    PrepareWorkspace(PrepareWorkspace),
    RestoreWorkspace(RestoreWorkspace),
    RemoveWorkspace(RemoveWorkspace),
    WorktreeThere(WorktreeThere),
    GitStatus(GitStatus),
    GitDiff(GitDiff),
    PullRequest(PullRequest),
    WatchBranches(WatchBranches),
    GithubRateLimit(GithubRateLimit),
    SetupWorkspace(SetupWorkspace),
    /// See [`CancelSetup`].
    CancelSetup,
    SetupLog(SetupLog),
    ArchiveScript(ArchiveScript),
    AttachmentWrite(AttachmentWrite),
    ListFiles(ListFiles),
    FolderRead(FolderRead),
    FileRead(FileRead),
    FileReadAt(FileReadAt),
    FileOpenAt(FileOpenAt),
    FilePages(FilePages),
    AgentStart(AgentSpec),
    AgentPrompt(AgentPrompt),
    AgentApproval(AgentApproval),
    AgentCancel(AgentCancel),
    AgentCompact(AgentCompact),
    AgentConfigure(AgentConfigure),
    AgentStop(AgentStop),
    TitleFor(TitleFor),
    AccountFor(AccountFor),
    ServeTools(ServeTools),
    /// Any request this build doesn't know, from a hub newer than it. A worker answers with
    /// [`ErrorCode::Unsupported`](crate::client::ErrorCode::Unsupported) and carries on, as a hub does a client's.
    #[serde(other)]
    Unknown,
}

/// Every reply, each read as the type of its own that a request names.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum WorkerResponse {
    Hello(HelloReply),
    RunningAgents(RunningAgentsReply),
    Ok,
    WorkspaceReady(WorkspaceReady),
    WorktreeThere(WorktreeThereReply),
    GitStatus(model::GitStatus),
    GitDiff(model::GitDiff),
    PullRequest(PullRequestReply),
    GithubRateLimit(RateLimit),
    SetupDone(SetupDone),
    SetupLog(SetupLogReply),
    AttachmentWritten(AttachmentWritten),
    Files(WorktreeFiles),
    FileContents(FileContents),
    FileOpened(FileOpened),
    FilePages(model::FilePages),
    AgentStarted(AgentStarted),
    /// The session has no running agent. The hub starts one and sends the request again.
    AgentNotRunning,
    Title(TitleReply),
    Account(AgentAccount),
    ToolsServing(ToolsServing),
}

pub type WorkerOutcome = Outcome<WorkerResponse>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The rest are each the variant that holds them, which the compiler checks.
    #[test]
    fn a_request_with_no_fields_is_sent_as_its_own_method() {
        let sent = |request: WorkerRequest| serde_json::to_string(&request).unwrap();

        assert_eq!(sent(Stop.into_request()), r#"{"method":"stop"}"#);
        assert_eq!(sent(RunningAgents.into_request()), r#"{"method":"running_agents"}"#);
        assert_eq!(sent(CancelSetup.into_request()), r#"{"method":"cancel_setup"}"#);
    }

    /// The rest each read the variant that holds them, which the compiler checks.
    #[test]
    fn a_reply_with_no_fields_reads_as_what_it_says_and_any_other_is_given_back() {
        let title = WorkerResponse::Title(TitleReply { title: "Fix the login bug".into() });

        assert_eq!(<()>::read(WorkerResponse::Ok), Ok(()));
        assert_eq!(<()>::read(WorkerResponse::AgentNotRunning), Err(WorkerResponse::AgentNotRunning));
        assert_eq!(Delivery::read(WorkerResponse::Ok), Ok(Delivery::Delivered));
        assert_eq!(Delivery::read(WorkerResponse::AgentNotRunning), Ok(Delivery::AgentNotRunning));
        assert_eq!(Delivery::read(title.clone()), Err(title.clone()));
        assert_eq!(model::GitStatus::read(title.clone()), Err(title));
    }
}
