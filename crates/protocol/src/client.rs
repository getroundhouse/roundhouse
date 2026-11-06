//! The platform API: what a desktop app, CLI or phone says to a hub.

use serde::{Deserialize, Serialize};

use crate::event::{ApprovalDecision, SessionEvent};
use crate::model::{
    AgentKind, DiffComment, DiffSide, GitDiff, GitStatus, PullRequest, Repo, RepoId, Seq, Session, SessionId,
    Workspace, WorkspaceId,
};
use crate::tail::{StreamDelta, Tail};

/// Replies to a [`ClientFrame`] carry its `id`, and can arrive in any order.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClientFrame {
    pub id: u64,
    pub request: ClientRequest,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ClientRequest {
    /// Must come first on a connection.
    Hello {
        protocol_version: u32,
        client: String,
    },
    HubStatus,
    /// Stops the hub and its workers. Agents are stopped and resume on the next prompt.
    HubShutdown,

    RepoAdd {
        path: String,
    },
    RepoList,

    /// Generates a name if none is given. Uses the repository's default branch if no base is given.
    WorkspaceCreate {
        repo_id: RepoId,
        name: Option<String>,
        base_ref: Option<String>,
    },
    WorkspaceList {
        include_archived: bool,
    },
    /// Fails if the worktree has uncommitted changes, unless `force` is set. The branch is kept.
    WorkspaceArchive {
        id: WorkspaceId,
        force: bool,
    },
    WorkspaceRestore {
        id: WorkspaceId,
    },
    WorkspaceStatus {
        id: WorkspaceId,
    },
    WorkspaceDiff {
        id: WorkspaceId,
    },
    /// The pull request for this workspace's branch. The reply is `None` if there isn't one, or if
    /// GitHub's CLI isn't available.
    WorkspacePullRequest {
        id: WorkspaceId,
    },
    CommentAdd {
        workspace_id: WorkspaceId,
        path: String,
        side: DiffSide,
        line: u32,
        text: String,
    },
    CommentList {
        workspace_id: WorkspaceId,
    },
    CommentDelete {
        id: String,
    },

    SessionCreate {
        workspace_id: WorkspaceId,
        agent: AgentKind,
        model: Option<String>,
        permission_mode: Option<String>,
    },
    SessionList {
        workspace_id: WorkspaceId,
    },
    SessionGet {
        id: SessionId,
    },
    /// Reads events from `from_seq` on. Returns at least one event if any exist, and stops at
    /// `limit` events or about `max_bytes`, so a huge event can't produce an unbounded reply.
    SessionRead {
        id: SessionId,
        from_seq: Seq,
        limit: u32,
        max_bytes: Option<u64>,
        #[serde(default)]
        include_raw: bool,
    },
    /// Sends every event from `from_seq` on, then keeps streaming as new ones are recorded. To
    /// catch up after reconnecting, subscribe from one past the last `seq` received. The client
    /// picks `subscription_id`, because events can arrive before the reply does.
    SessionSubscribe {
        id: SessionId,
        from_seq: Seq,
        subscription_id: u64,
    },
    /// Works for any kind of subscription.
    SessionUnsubscribe {
        subscription_id: u64,
    },
    /// Sends a [`HubNotice`] whenever a record a sidebar shows changes. List first, then subscribe,
    /// and list again on `Resync`.
    FeedSubscribe {
        subscription_id: u64,
    },
    /// Resolves once the prompt is recorded. Sending the same `client_msg_id` twice records it once,
    /// so retrying after a disconnect can't send it twice. A retry only records: if the hub stopped
    /// before delivering the original, the session shows as interrupted, and the prompt needs sending
    /// again with a new ID.
    SessionPrompt {
        id: SessionId,
        text: String,
        client_msg_id: Option<String>,
    },
    SessionApprove {
        id: SessionId,
        approval_id: String,
        decision: ApprovalDecision,
    },
    SessionCancel {
        id: SessionId,
    },
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
    Repo(Repo),
    Repos {
        repos: Vec<Repo>,
    },
    Workspace(Workspace),
    Workspaces {
        workspaces: Vec<Workspace>,
    },
    GitStatus(GitStatus),
    GitDiff(GitDiff),
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
    PromptRecorded {
        seq: Seq,
        duplicate: bool,
    },
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
    /// A worker or provider failed.
    Worker,
    Internal,
    #[serde(other)]
    Unknown,
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

/// A record changed. Carries the whole record, so a client only ever replaces its copy.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "notice", rename_all = "snake_case")]
pub enum HubNotice {
    Repo(Repo),
    Workspace(Workspace),
    Session(Session),
    /// This client fell behind and missed some. List everything again.
    Resync,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
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
