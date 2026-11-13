use super::{
    admin, agent, attachments, comments, feed, hello, log, new_workspace, records, request, sessions, workspaces,
};
use crate::attachment::Attachment;
use crate::event::SessionEvent;
use crate::find::Found;
use crate::model::{
    Account, DiffComment, FileContents, FileOpened, FilePages, GitDiff, GitStatus, HubSettings, NewWorkspaceDraft,
    Records, Repo, Session, SessionAccount, TitleError, Workspace, WorktreeFiles,
};
use crate::tail::{StreamDelta, Tail};
use feed::HubNotice;
use hello::HelloReply;
use request::ClientRequest;
use serde::{Deserialize, Serialize};

/// The client protocol's version, apart from the worker protocol's, so a break in one doesn't break the other. It
/// changes only for a real break, as "Changing the protocol" in `docs/architecture.md` says.
pub const CLIENT_PROTOCOL_VERSION: u32 = 3;

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
    Workspace(Workspace),
    Workspaces(workspaces::Workspaces),
    GitStatus(GitStatus),
    GitDiff(GitDiff),
    SetupLog(workspaces::SetupLog),
    PullRequest(workspaces::PullRequestReply),
    Comment(DiffComment),
    Comments(comments::Comments),
    Session(Session),
    Sessions(sessions::Sessions),
    Events(log::Events),
    Subagents(records::Subagents),
    PlacedEvents(log::PlacedEvents),
    Prompts(log::Prompts),
    Compactions(log::Compactions),
    Found(Found),
    PromptRecorded(agent::PromptRecorded),
    Attachment(Attachment),
    AttachmentData(attachments::AttachmentData),
    SessionAccount(SessionAccount),
    Account(Account),
    WorktreeFiles(WorktreeFiles),
    FileContents(FileContents),
    FileOpened(FileOpened),
    FilePages(FilePages),
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
}

impl HubStatus {
    /// Whether nothing would notice the hub stopping: nothing is running on it, and whoever asked for this is its one
    /// client, with their request the one thing it's doing.
    pub fn is_idle(&self) -> bool {
        self.running_sessions == 0 && self.running_setups == 0 && self.requests <= 1 && self.clients <= 1
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The first request wasn't `Hello`, or the versions don't match.
    VersionMismatch,
    NotFound,
    InvalidRequest,
    /// The request is valid but the current state doesn't allow it.
    Conflict,
    /// Asking again with `force` would go ahead, at the cost the message describes.
    NeedsForce,
    /// A name is in use, or the branch, folder of branches or directory it would need is.
    NameTaken,
    /// A worker or provider failed.
    Worker,
    /// What it needs isn't there, such as GitHub's CLI, a login to it, a GitHub remote, or what's left of GitHub's
    /// hourly limit, so asking again won't help until that changes.
    Unavailable,
    Internal,
    /// The hub, worker or provider adapter doesn't know the request, or can't read it, because it's older than
    /// whoever asked. The caller hides, disables or does without whatever needed it.
    Unsupported,
    #[serde(other)]
    Unknown,
}

/// An error and everything that caused it, on one line. [`ApiError`] only holds text, so this is what
/// keeps the cause when a lower error crosses a process.
pub fn error_chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    /// The refusal of a request from a newer peer, which `reader`, such as "this hub", of the build whose version is
    /// `build`, doesn't know, or knows but can't read, such as one carrying a value it doesn't know. Its caller does
    /// without either way.
    pub fn unsupported_request(reader: &str, build: &str, unreadable: Option<&serde_json::Error>) -> Self {
        let message = match unreadable {
            None => format!("{reader}, version {build}, doesn't know that request"),
            Some(error) => format!("{reader}, version {build}, can't read that request: {error}"),
        };
        Self::new(ErrorCode::Unsupported, message)
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

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

/// A title someone typed that the hub refuses, with a message they can read.
impl From<TitleError> for ApiError {
    fn from(e: TitleError) -> Self {
        ApiError::new(ErrorCode::InvalidRequest, e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_chain_names_every_cause_in_order() {
        #[derive(Debug)]
        struct Outer(std::io::Error);
        impl std::fmt::Display for Outer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("opening the log")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let error = Outer(std::io::Error::other("disk full"));

        assert_eq!(error_chain(&error), "opening the log: disk full");
    }

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
