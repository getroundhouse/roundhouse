//! The platform API: what a desktop app, CLI or phone says to a hub.

use serde::{Deserialize, Serialize};

use crate::attachment::{Attachment, AttachmentBytes};
use crate::event::SessionEvent;
use crate::find::Found;
use crate::model::{
    Account, DiffComment, FileContents, GitDiff, GitStatus, HubSettings, Place, PromptLine, PullRequest, RecordKey,
    Records, Repo, Rev, Seq, Session, SessionAccount, Subagent, TitleError, Workspace, WorktreeFiles,
};
use crate::tail::{StreamDelta, Tail};

mod request;

pub use request::ClientRequest;

/// Replies to a [`ClientFrame`] carry its `id`, and can arrive in any order.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClientFrame {
    pub id: u64,
    pub request: ClientRequest,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ClientResponse {
    Hello {
        protocol_version: u32,
        hub_version: String,
    },
    Ok,
    HubStatus(HubStatus),
    HubSettings(HubSettings),
    Repo(Repo),
    Repos {
        repos: Vec<Repo>,
    },
    Records(Records),
    Workspace(Workspace),
    Workspaces {
        workspaces: Vec<Workspace>,
    },
    GitStatus(GitStatus),
    GitDiff(GitDiff),
    SetupLog {
        text: String,
    },
    PullRequest {
        pull_request: Option<PullRequest>,
    },
    Comment(DiffComment),
    Comments {
        comments: Vec<DiffComment>,
    },
    Session(Session),
    Sessions {
        sessions: Vec<Session>,
    },
    Events {
        events: Vec<SessionEvent>,
        /// The session's newest event, so a reader knows how far the log goes.
        last_seq: Seq,
    },
    /// A session's subagents, oldest first.
    Subagents {
        subagents: Vec<Subagent>,
        /// The hub's newest revision when they were read, as a list of every record carries.
        rev: Rev,
    },
    /// The agent's own events, or one subagent's.
    PlacedEvents {
        events: Vec<SessionEvent>,
        /// The place of the newest of them, so a reader knows how far they go. 0 when there are none.
        last_place: Place,
    },
    Prompts {
        prompts: Vec<PromptLine>,
    },
    Found(Found),
    PromptRecorded {
        seq: Seq,
        duplicate: bool,
    },
    Attachment(Attachment),
    AttachmentData {
        data: AttachmentBytes,
    },
    SessionAccount(SessionAccount),
    WorktreeFiles(WorktreeFiles),
    FileContents(FileContents),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubStatus {
    pub pid: u32,
    pub hub_version: String,
    pub protocol_version: u32,
    pub started_at_ms: i64,
    pub workers: u32,
    pub running_sessions: u32,
    /// Connected clients, including the one asking.
    pub clients: u32,
    /// A client started it because it needed one, rather than someone choosing to run it. Only a hub
    /// like this can be restarted from a client.
    #[serde(default)]
    pub on_demand: bool,
    /// Which build it is, if it's one that has an order. See [`crate::BUILD_NUMBER`].
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
    /// A name is in use, or the branch or directory it would need is.
    NameTaken,
    /// A worker or provider failed.
    Worker,
    /// What it needs isn't there, such as GitHub's CLI, a login to it, a GitHub remote, or what's left of GitHub's
    /// hourly limit, so asking again won't help until that changes.
    Unavailable,
    Internal,
    /// This hub doesn't know the request, usually because it's older than the client. The client
    /// hides or disables whatever needed it.
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

/// A record changed. Carries the whole record and its revision, so a client only ever replaces its
/// copy with a newer one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "notice", rename_all = "snake_case")]
pub enum HubNotice {
    Repo(Repo),
    Workspace(Workspace),
    Session(Session),
    Comment(DiffComment),
    HubSettings(HubSettings),
    Account(Account),
    /// A subagent's record changed. A client keeps those of the sessions it's showing, which it lists with
    /// `SessionSubagents`.
    Subagent(Subagent),
    /// The record is gone, as of `rev`.
    Removed {
        record: RecordKey,
        rev: Rev,
    },
    /// This client fell behind and missed some. List everything again, and the subagents of each session it's
    /// showing, which aren't among everything.
    Resync,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
#[expect(
    clippy::large_enum_variant,
    reason = "a reply is decoded once and handed straight to its caller, and boxing it would stop callers matching through it"
)]
pub enum ReplyOutcome {
    Response(ClientResponse),
    Error(ApiError),
}

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
}
