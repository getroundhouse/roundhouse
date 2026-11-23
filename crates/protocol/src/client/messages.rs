use super::{
    ApiError, admin, agent, attachments, comments, feed, github_apps, hello, log, new_workspace, proposals, records,
    request, sessions, workspaces,
};
use crate::attachment::Attachment;
use crate::event::SessionEvent;
use crate::find::Found;
use crate::model::{
    Account, DiffComment, FileContents, FileDiff, FileOpened, FilePages, FileRange, GhLogin, GitDiff, GitStatus,
    GithubAccount, GithubApp, GithubSignIn, HubSettings, MachineLogins, NewWorkspaceDraft, Proposal, Records, Repo,
    Session, SessionAccount, Terminal, Workspace, WorktreeFiles,
};
use crate::tail::{StreamDelta, Tail};
use crate::terminal::{
    TerminalBoundsReply, TerminalFitReply, TerminalPicture, TerminalRowsReply, TerminalTextReply, TerminalUpdate,
};
use crate::versions::Versions;
use feed::HubNotice;
use hello::HelloReply;
use request::ClientRequest;
use serde::{Deserialize, Serialize};

/// The client protocol's version, apart from the worker protocol's, so a break in one doesn't break the other. It
/// changes only for a real break, as `docs/principles/versions.md` says. Version 4 is the one a client follows
/// everything through [`Subscribe`](super::subscriptions::Subscribe) alone in.
pub const CLIENT_PROTOCOL_VERSION: u32 = 4;

/// The versions of the client protocol a hub of this build speaks: its own, and the one before it, for a client of an
/// older build, as `docs/principles/versions.md` says. Version 3 is the one a client follows through a request of its
/// own for each of the hub's records, a session, a terminal and what it shows of GitHub. Nothing a hub sends a client
/// of version 3 differs from what it sent it before version 4.
///
/// Both are written out, rather than taken from [`CLIENT_PROTOCOL_VERSION`], since at a break the hub speaks the new
/// version a merge before clients do, so a client of the build that first speaks it can talk to the hub of the build
/// before. A hub has to speak its own build's clients' version, which the assertion below holds.
pub const HUB_SPEAKS: Versions = Versions { oldest: 3, newest: 4 };

const _: () = assert!(HUB_SPEAKS.speaks(CLIENT_PROTOCOL_VERSION), "a hub speaks its own build's clients' version");

/// A request to a hub, which names what the hub answers it with.
pub trait Request {
    type Reply: Reply;

    /// It among every other request, as it goes on the wire.
    fn into_request(self) -> ClientRequest;
}

/// What a hub answers a request with, read from the response it came as. One type can answer many requests.
pub trait Reply: Sized {
    /// Gives the response back when it's of another kind, for whoever asked to say so.
    #[expect(
        clippy::result_large_err,
        reason = "a reply is moved once, as it's read, so boxing one of another kind would only add an allocation"
    )]
    fn read(response: ClientResponse) -> Result<Self, ClientResponse>;
}

/// What a request that only does something is answered with: `Ok`, once the hub has done it, or, for one whose work goes
/// on, such as stopping the hub or following a log, once it has begun.
impl Reply for () {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Ok = response else { return Err(response) };
        Ok(())
    }
}

/// Replies to a [`ClientFrame`] carry its `id`, and can arrive in any order.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClientFrame {
    pub id: u64,
    pub request: ClientRequest,
}

/// Every reply, each read as the type of its own that a request names.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ClientResponse {
    Paired(super::devices::Paired),
    Devices(super::devices::Devices),
    Pairing(crate::pairing::Pairing),
    Device(crate::pairing::Device),
    Hello(HelloReply),
    Ok,
    HubStatus(HubStatus),
    HubSettings(HubSettings),
    NewWorkspaceDraft(NewWorkspaceDraft),
    /// Boxed, since it holds a workspace and a session, so every other response isn't as large.
    NewWorkspaceMade(Box<new_workspace::NewWorkspaceMade>),
    Repo(Repo),
    Repos(records::Repos),
    Records(Records),
    Agents(admin::Agents),
    ModelList(crate::model::ModelList),
    Workspace(Workspace),
    Workspaces(workspaces::Workspaces),
    GitStatus(GitStatus),
    GitDiff(GitDiff),
    ChangedFilesPage(crate::model::ChangedFilesPage),
    FileDiff(FileDiff),
    SetupLog(workspaces::SetupLog),
    PullRequest(workspaces::PullRequestReply),
    Comment(DiffComment),
    Comments(comments::Comments),
    Session(Session),
    Sessions(sessions::Sessions),
    Events(log::Events),
    Subagents(records::Subagents),
    Proposals(records::Proposals),
    RepliesHeard(records::RepliesHeard),
    References(super::references::References),
    Proposal(Proposal),
    ProposalRevisions(proposals::ProposalRevisions),
    PlacedEvents(log::PlacedEvents),
    Prompts(log::Prompts),
    Compactions(log::Compactions),
    Found(Found),
    PromptRecorded(agent::PromptRecorded),
    Attachment(Attachment),
    AttachmentData(attachments::AttachmentData),
    SessionAccount(SessionAccount),
    Account(Account),
    GithubAccount(GithubAccount),
    GithubSignIn(GithubSignIn),
    GhLogin(GhLogin),
    MachineLogins(MachineLogins),
    GithubApp(GithubApp),
    GithubAppManifest(github_apps::GithubAppManifest),
    WorktreeFiles(WorktreeFiles),
    FileContents(FileContents),
    FileOpened(FileOpened),
    FilePages(FilePages),
    FileRange(FileRange),
    Terminal(Terminal),
    TerminalRows(TerminalRowsReply),
    TerminalBounds(TerminalBoundsReply),
    TerminalText(TerminalTextReply),
    TerminalFit(TerminalFitReply),
    Subscribed(super::subscriptions::Subscribed),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubStatus {
    pub pid: u32,
    pub hub_version: String,
    pub protocol_version: u32,
    pub started_at_ms: i64,
    pub workers: u32,
    /// Sessions whose agent is at work on what stopping the hub would cut short, as
    /// [`crate::model::SessionStatus::counts_as_running`] says: on a turn, or in the background on anything but waits
    /// on the pull request, which the next hub keeps. A hub from before waits counts every session at work.
    pub running_sessions: u32,
    /// Connected clients, including the one asking.
    pub clients: u32,
    /// A client started it because it needed one, rather than someone choosing to run it. Only a hub
    /// like this can be restarted from a client.
    #[serde(default)]
    pub on_demand: bool,
    /// What started it, which [`HubStatus::start`] reads. `None` from a hub from before this, which says only
    /// `on_demand`. A new kind of start would leave a client from before it unable to read the status, across a break
    /// too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_by: Option<HubStart>,
    /// Which build it is, if it's one that has an order. See [`crate::Build::number`].
    #[serde(default)]
    pub build_number: Option<u64>,
    /// Workspaces whose setup script is running.
    #[serde(default)]
    pub running_setups: u32,
    /// Requests it's still working on, including the one asking. These carry on after their client has gone,
    /// such as creating or archiving a workspace.
    #[serde(default)]
    pub requests: u32,
    /// Whether it's keeping its machine from going to sleep by itself right now, as it does while there's work unless
    /// it's been told not to.
    #[serde(default)]
    pub keeping_awake: bool,
    /// Whether restarting it with `hub_restart` leaves its workers running, with their agents. A hub from before
    /// that stops them, so a client replaces it only once nothing is running on it.
    #[serde(default)]
    pub keeps_workers: bool,
    /// The oldest version of the client protocol it speaks, as well as `protocol_version`, the newest. `None` from a
    /// hub from before this, which speaks only the one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oldest_protocol_version: Option<u32>,
    /// The oldest version of the worker protocol among the workers it's running, counting its own, which every worker it
    /// starts speaks. A hub that doesn't speak it would end those workers as it started, so the app takes no update whose
    /// hub doesn't. Its own counts even with no worker running, since the hub goes on after the app quits, and starts a
    /// worker of its own build whenever something asks for one, such as an agent whose wait on its pull request ends.
    /// `None` from a hub from before this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oldest_running_worker_protocol: Option<u32>,
    /// Terminals open in its workspaces, or being opened, whose shells stopping their workers would end. A hub from
    /// before terminals has none.
    #[serde(default)]
    pub open_terminals: u32,
}

/// What started a hub, which says who may restart it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HubStart {
    /// Someone ran it themselves, so it's theirs to stop or update.
    ByHand,
    /// A client started it because it needed one, so a client may restart it.
    OnDemand,
    /// The machine's service manager, since `roundhouse hub install` made it a service, which starts it at boot and
    /// again after a failure.
    AsService,
}

impl HubStatus {
    pub fn start(&self) -> HubStart {
        self.started_by.unwrap_or(if self.on_demand { HubStart::OnDemand } else { HubStart::ByHand })
    }

    /// Whether stopping the hub would cut nothing off: nothing is running on it, no terminal is open, and the request
    /// asking for this is the one thing it's doing. Other clients don't count. One that's only connected loses nothing,
    /// since it connects again to the next hub, and what's typed in it is kept by the hub, while counting it would keep
    /// an older hub, and the update waiting on it, for as long as an app on another device stays open.
    pub fn is_idle(&self) -> bool {
        self.running_sessions == 0 && self.running_setups == 0 && self.open_terminals == 0 && self.requests <= 1
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum HubFrame {
    Reply {
        id: u64,
        #[serde(flatten)]
        outcome: ReplyOutcome,
    },
    Event {
        subscription_id: u64,
        event: SessionEvent,
    },
    /// Replaces the client's streaming tail. Sent once the subscription has caught up with the
    /// log, and again if the client falls too far behind the live stream.
    Tail {
        subscription_id: u64,
        tail: Tail,
    },
    Delta {
        subscription_id: u64,
        delta: StreamDelta,
    },
    Notice {
        subscription_id: u64,
        notice: HubNotice,
    },
    /// What the hub knows now of one of what a [`super::github::GithubSubscribe`] shows.
    Github {
        subscription_id: u64,
        update: super::github::GithubUpdate,
    },
    /// That the shell of the terminal a `TerminalWatch` follows ended, after which nothing more comes.
    Terminal {
        subscription_id: u64,
        update: TerminalUpdate,
    },
    /// A frame of what the terminal a `TerminalWatch` follows shows, the newest the hub has whenever the last has been
    /// written.
    TerminalPicture {
        subscription_id: u64,
        picture: TerminalPicture,
    },
    /// Where what a [`super::subscriptions::Subscribe`] subscription follows stands, as [`super::listed::Listed`] says.
    /// Sent first, and again whenever the subscription has fallen behind.
    Listed {
        subscription_id: u64,
        listed: super::listed::Listed,
    },
}

/// What a reply says: the answer, or why there's none. Every protocol's replies take this form, flattened into the
/// frame beside the ID they answer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome<T> {
    Response(T),
    Error(ApiError),
}

impl<T> From<Result<T, ApiError>> for Outcome<T> {
    fn from(result: Result<T, ApiError>) -> Self {
        match result {
            Ok(response) => Outcome::Response(response),
            Err(error) => Outcome::Error(error),
        }
    }
}

impl<T> Outcome<T> {
    pub fn into_result(self) -> Result<T, ApiError> {
        match self {
            Outcome::Response(response) => Ok(response),
            Outcome::Error(error) => Err(error),
        }
    }
}

pub type ReplyOutcome = Outcome<ClientResponse>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The rest are each the variant that holds them, which the compiler checks.
    #[test]
    fn a_request_with_no_fields_is_sent_as_its_own_method() {
        let sent = |request: ClientRequest| serde_json::to_string(&request).unwrap();

        assert_eq!(sent(admin::HubStatus.into_request()), r#"{"method":"hub_status"}"#);
        assert_eq!(sent(admin::HubShutdown.into_request()), r#"{"method":"hub_shutdown"}"#);
        assert_eq!(sent(admin::HubShutdownIfIdle.into_request()), r#"{"method":"hub_shutdown_if_idle"}"#);
        assert_eq!(sent(admin::HubRestart.into_request()), r#"{"method":"hub_restart"}"#);
        assert_eq!(sent(admin::AgentList.into_request()), r#"{"method":"agent_list"}"#);
        assert_eq!(sent(records::RepoList.into_request()), r#"{"method":"repo_list"}"#);
        assert_eq!(sent(records::RecordsList.into_request()), r#"{"method":"records_list"}"#);
    }

    /// The rest each read the variant that holds them, which the compiler checks.
    #[test]
    fn a_reply_with_no_fields_reads_as_what_it_says_and_any_other_is_given_back() {
        let log = ClientResponse::SetupLog(workspaces::SetupLog { text: "Running the setup script".into() });

        assert_eq!(<()>::read(ClientResponse::Ok), Ok(()));
        assert_eq!(<()>::read(log.clone()), Err(log.clone()));
        assert_eq!(GitStatus::read(ClientResponse::Ok), Err(ClientResponse::Ok));
        assert_eq!(Session::read(log.clone()), Err(log));
    }
}
