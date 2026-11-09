//! Records the hub owns and serves.

use serde::{Deserialize, Serialize};

/// Declares ID types: text that's only compared and passed along, with a type each so that one
/// can't be passed as another. They're plain strings on the wire and in storage.
macro_rules! ids {
    ($($(#[$doc:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$doc])*
        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self(id)
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self(id.to_string())
            }
        }
    )+};
}

ids!(
    RepoId,
    WorkspaceId,
    SessionId,
    CommentId,
    /// The agent's own ID for an approval it asked for.
    ApprovalId,
    /// Where a workspace's worker runs, in its provider's terms.
    InstanceId,
    /// The agent's own ID for a conversation, which is what resumes it.
    AgentSessionId,
    /// Chosen by a client, so that retrying a prompt can't record it twice.
    ClientMessageId,
    /// Chosen by a client, so that retrying a request that creates something can't create it twice.
    ClientRequestId,
    /// The agent's own ID for a message it's sending.
    MessageId,
    /// The agent's own ID for one use of a tool.
    ToolUseId,
    /// The agent's own ID for work it runs in the background.
    TaskId,
);

/// Position in a session's event log. The first event is 1, and there are no gaps.
pub type Seq = u64;

/// Declares counters a hub hands out: numbers that only go up, with a type each so that one can't be
/// passed as another. They're plain numbers on the wire and in storage.
macro_rules! counters {
    ($($(#[$meta:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$meta])*
        #[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    )+};
}

counters!(
    /// When a record last changed, from one counter per hub, so a client can tell which of two copies is
    /// newer. A hub that doesn't keep revisions sends none, which reads as 0, and one that does never
    /// uses 0.
    #[derive(Default)]
    Rev,
    /// One launch of an agent. The hub picks it before the launch, and it never repeats, even across
    /// hub restarts, so whatever a run produced can always be told apart from its replacement's. A
    /// later launch has a higher one.
    RunId,
);

/// A `seq` as a position in a list of events.
#[expect(
    clippy::cast_possible_truncation,
    reason = "only 64-bit targets are supported, where this can't lose anything"
)]
pub const fn seq_index(seq: Seq) -> usize {
    seq as usize
}

/// Which record, of which kind. A kind this build doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordKey {
    Repo {
        id: RepoId,
    },
    Workspace {
        id: WorkspaceId,
    },
    Session {
        id: SessionId,
    },
    Comment {
        id: CommentId,
    },
    #[serde(other)]
    Unknown,
}

/// Every record a hub holds, read at one moment.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Records {
    pub repos: Vec<Repo>,
    pub workspaces: Vec<Workspace>,
    pub sessions: Vec<Session>,
    pub comments: Vec<DiffComment>,
    /// The hub's newest revision when this was read. A record missing from the list is gone, unless
    /// a client holds a copy newer than this.
    pub rev: Rev,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    pub id: RepoId,
    pub name: String,
    pub path: String,
    pub created_at_ms: i64,
    #[serde(default)]
    pub rev: Rev,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Creating,
    Ready,
    Archived,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub repo_id: RepoId,
    pub name: String,
    pub branch: String,
    /// What the branch was created from, such as `origin/main`. Diffs are taken against this.
    pub base_ref: Option<String>,
    /// The worktree's path on the worker's machine. Empty until the worker has created it.
    pub path: Option<String>,
    pub status: WorkspaceStatus,
    pub provider: String,
    pub instance_id: InstanceId,
    pub created_at_ms: i64,
    pub archived_at_ms: Option<i64>,
    pub setup: SetupStatus,
    /// The first of ten ports set aside for this workspace, so workspaces don't collide.
    pub port_base: u16,
    #[serde(default)]
    pub rev: Rev,
}

/// Where a workspace's setup has got to. Copying local files counts, even with no script to run.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SetupStatus {
    #[default]
    NotStarted,
    Running,
    Succeeded,
    /// The workspace is still usable. The code is `None` if the script was stopped, or never finished.
    Failed {
        code: Option<i32>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Claude,
    #[serde(other)]
    Unknown,
}

impl AgentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Unknown => "unknown",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Nothing is happening, and the next prompt starts a turn.
    Idle,
    Running,
    WaitingApproval,
    /// The agent went away mid-turn. The next prompt resumes the conversation.
    Interrupted,
    /// The turn has ended, and work the agent started in the background, such as a command, is still going. The
    /// agent picks up again by itself when it finishes, and a prompt starts a turn meanwhile.
    Background,
    #[serde(other)]
    Unknown,
}

impl SessionStatus {
    /// Whether the agent is at work, in a turn or in the background, so stopping it would cut something short.
    pub fn is_active(self) -> bool {
        match self {
            SessionStatus::Running | SessionStatus::WaitingApproval | SessionStatus::Background => true,
            SessionStatus::Idle | SessionStatus::Interrupted | SessionStatus::Unknown => false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub workspace_id: WorkspaceId,
    pub agent: AgentKind,
    pub model: Option<String>,
    /// Passed through to the agent. `None` leaves the choice to the user's own agent settings.
    pub permission_mode: Option<String>,
    pub status: SessionStatus,
    /// The newest event when this copy was read, and `updated_at_ms` its time. Every event moves them,
    /// and the feed doesn't announce that, or its revision, so the log says how far a session has got.
    pub last_seq: Seq,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// The start of the first prompt. `None` until there's been one.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub closed_at_ms: Option<i64>,
    /// The event that ended the newest turn, which makes what the agent said then its reply. 0 until a turn has
    /// ended. It moves with a change of status, so the feed announces it.
    #[serde(default)]
    pub reply_seq: Seq,
    /// How far the person has read, so a reply past it is unread. It's theirs rather than the session's, which a
    /// hub with several people would keep for each.
    #[serde(default)]
    pub read_seq: Seq,
    #[serde(default)]
    pub rev: Rev,
}

impl Session {
    /// The agent has replied since the person last read this chat.
    pub fn unread(&self) -> bool {
        self.reply_seq > self.read_seq
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct GitStatus {
    pub branch: Option<String>,
    pub head: Option<String>,
    /// Against the branch's upstream. Workspace branches have none until they're pushed, so these
    /// don't say how far a branch is from its base.
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<GitFile>,
    /// On a detached HEAD, with commits that no branch or tag holds.
    #[serde(default)]
    pub unreferenced_commits: bool,
}

impl GitStatus {
    /// What removing this worktree would lose, in words for a person. Commits on a branch aren't at
    /// risk, because a branch outlives its worktree.
    pub fn loss_if_removed(&self) -> Option<String> {
        if !self.files.is_empty() {
            let count = self.files.len();
            return Some(format!("{count} uncommitted change(s) would be lost. Commit them, or archive with force"));
        }
        self.unreferenced_commits.then(|| {
            "Commits on a detached HEAD, on no branch, would be lost. Put them on a branch, or archive with force"
                .to_string()
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GitFile {
    pub path: String,
    /// Set for renames and copies.
    pub orig_path: Option<String>,
    /// Two-letter porcelain status, such as `.M`, `A.` or `??`.
    pub status: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct GitDiff {
    /// The commit the diff is taken from: the merge base with the workspace's base ref.
    pub base_commit: Option<String>,
    pub patch: String,
    /// True when untracked files were left out to keep the response bounded.
    pub truncated: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffSide {
    Old,
    New,
    #[serde(other)]
    Unknown,
}

/// A review comment on a changed line, waiting to be sent to the agent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DiffComment {
    pub id: CommentId,
    pub workspace_id: WorkspaceId,
    pub path: String,
    pub side: DiffSide,
    pub line: u32,
    pub text: String,
    pub created_at_ms: i64,
    #[serde(default)]
    pub rev: Rev,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
    pub title: String,
    /// As GitHub reports it: `OPEN`, `CLOSED` or `MERGED`.
    pub state: String,
    pub is_draft: bool,
}
