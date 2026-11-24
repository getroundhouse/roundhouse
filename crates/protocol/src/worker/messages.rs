use super::{
    agents, archived, children, connection, git_credential, one_off, previous, repo_url, terminals, tools, workspaces,
    worktree,
};
use crate::client::Outcome;
use crate::model;
use crate::model::{AgentAccount, FileContents, FileOpened, WorktreeFiles};
use crate::terminal::{TerminalBoundsReply, TerminalFitReply, TerminalRowsReply, TerminalTextReply};
use crate::versions::Versions;
#[cfg(test)]
use agents::Delivery;
use agents::{AgentControl, AgentSpec, AgentStarted, AgentStop};
use archived::{ArchivedGit, CaptureArchive};
use children::{AddChildWorktree, ChildWorktreeAdded, WatchChildBranches};
use connection::{Hello, HelloReply, Received, RunningAgentsReply};
#[cfg(test)]
use connection::{RunningAgents, Stop};
use git_credential::GitCredentialSet;
use one_off::{AccountFor, TitleFor, TitleReply};
use repo_url::{ReadRepoUrl, RepoUrlReply};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use terminals::RunningTerminals;
use terminals::{
    RunningTerminalsReply, TerminalBounds, TerminalFit, TerminalRows, TerminalStart, TerminalStop, TerminalText,
    TerminalTyped, TerminalWatch,
};
use tools::{ServeTools, ToolsServing};
#[cfg(test)]
use workspaces::CancelSetup;
use workspaces::{
    ArchiveScript, NamesGitHas, NamesGitHasReply, PrepareWorkspace, RemoveWorkspace, RestoreWorkspace, SetupDone,
    SetupLog, SetupLogReply, SetupWorkspace, WorkspaceReady, WorktreeThere, WorktreeThereReply,
};
use worktree::{
    AttachmentWrite, AttachmentWritten, DiffFile, FileOpenAt, FilePages, FileRead, FileReadAt, FileReadRange,
    FolderRead, GitDiff, GitStatus, ListChangedFiles, ListFiles, SummarizeChanges, WatchBranches, WatchChanges,
};

/// The worker protocol's version, apart from the client protocol's, so a break in one doesn't break the other. It
/// changes only for a real break, as `docs/principles/versions.md` says.
pub const WORKER_PROTOCOL_VERSION: u32 = 6;

/// The versions of this protocol a hub of this build speaks: its own, and the one before it, for a worker of an older
/// build, as `docs/principles/versions.md` says. The update feed says it of each release, so an app can take one whose
/// hub speaks every worker at work.
pub const HUB_SPEAKS: Versions = Versions { oldest: previous::VERSION, newest: WORKER_PROTOCOL_VERSION };

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
    /// See [`Stop`](super::Stop).
    Stop,
    /// See [`RunningAgents`](super::RunningAgents).
    RunningAgents,
    Received(Received),
    NamesGitHas(NamesGitHas),
    PrepareWorkspace(PrepareWorkspace),
    CaptureArchive(CaptureArchive),
    RestoreWorkspace(RestoreWorkspace),
    RemoveWorkspace(RemoveWorkspace),
    WorktreeThere(WorktreeThere),
    AddChildWorktree(AddChildWorktree),
    GitStatus(GitStatus),
    GitDiff(GitDiff),
    SummarizeChanges(SummarizeChanges),
    ListChangedFiles(ListChangedFiles),
    DiffFile(DiffFile),
    WatchBranches(WatchBranches),
    WatchChanges(WatchChanges),
    WatchChildBranches(WatchChildBranches),
    SetupWorkspace(SetupWorkspace),
    /// See [`CancelSetup`](super::CancelSetup).
    CancelSetup,
    SetupLog(SetupLog),
    ArchiveScript(ArchiveScript),
    AttachmentWrite(AttachmentWrite),
    ListFiles(ListFiles),
    FolderRead(FolderRead),
    FileRead(FileRead),
    FileReadRange(FileReadRange),
    FileReadAt(FileReadAt),
    FileOpenAt(FileOpenAt),
    FilePages(FilePages),
    AgentStart(AgentSpec),
    AgentControl(AgentControl),
    AgentStop(AgentStop),
    TitleFor(TitleFor),
    AccountFor(AccountFor),
    ServeTools(ServeTools),
    GitCredentialSet(GitCredentialSet),
    ReadRepoUrl(ReadRepoUrl),
    TerminalStart(TerminalStart),
    TerminalTyped(TerminalTyped),
    TerminalFit(TerminalFit),
    TerminalRows(TerminalRows),
    TerminalBounds(TerminalBounds),
    TerminalText(TerminalText),
    TerminalWatch(TerminalWatch),
    TerminalStop(TerminalStop),
    /// See [`RunningTerminals`](super::RunningTerminals).
    RunningTerminals,
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
    NamesGitHas(NamesGitHasReply),
    WorkspaceReady(WorkspaceReady),
    ArchivedGit(ArchivedGit),
    WorktreeThere(WorktreeThereReply),
    ChildWorktreeAdded(ChildWorktreeAdded),
    GitStatus(model::GitStatus),
    GitDiff(model::GitDiff),
    ChangeSummary(model::ChangeSummary),
    ChangedFilesPage(model::ChangedFilesPage),
    FileDiff(model::FileDiff),
    SetupDone(SetupDone),
    SetupLog(SetupLogReply),
    AttachmentWritten(AttachmentWritten),
    Files(WorktreeFiles),
    FileContents(FileContents),
    FileOpened(FileOpened),
    FilePages(model::FilePages),
    FileRange(model::FileRange),
    AgentStarted(AgentStarted),
    /// The session has no running agent. The hub starts one and sends the request again.
    AgentNotRunning,
    /// The agent won't take the prompt while its turn runs, so it took nothing, and the hub sends it once the turn ends.
    AfterTheTurn,
    Title(TitleReply),
    Account(AgentAccount),
    ToolsServing(ToolsServing),
    RepoUrl(RepoUrlReply),
    TerminalRows(TerminalRowsReply),
    TerminalBounds(TerminalBoundsReply),
    TerminalText(TerminalTextReply),
    TerminalFit(TerminalFitReply),
    RunningTerminals(RunningTerminalsReply),
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
        assert_eq!(sent(RunningTerminals.into_request()), r#"{"method":"running_terminals"}"#);
    }

    /// The rest each read the variant that holds them, which the compiler checks.
    #[test]
    fn a_reply_with_no_fields_reads_as_what_it_says_and_any_other_is_given_back() {
        let title = WorkerResponse::Title(TitleReply { title: "Fix the login bug".into() });

        assert_eq!(<()>::read(WorkerResponse::Ok), Ok(()));
        assert_eq!(<()>::read(WorkerResponse::AgentNotRunning), Err(WorkerResponse::AgentNotRunning));
        assert_eq!(Delivery::read(WorkerResponse::Ok), Ok(Delivery::Delivered));
        assert_eq!(Delivery::read(WorkerResponse::AgentNotRunning), Ok(Delivery::AgentNotRunning));
        assert_eq!(Delivery::read(WorkerResponse::AfterTheTurn), Ok(Delivery::AfterTheTurn));
        assert_eq!(Delivery::read(title.clone()), Err(title.clone()));
        assert_eq!(model::GitStatus::read(title.clone()), Err(title));
    }
}
