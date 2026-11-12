//! Records the hub owns and serves.

use serde::{Deserialize, Serialize};

mod account;
mod cache;
mod context;
mod draft;
mod effort;
mod files;
mod hub_settings;
mod model_name;
mod one_line;
mod permission_mode;
mod prompt_line;
mod pull_request;
mod queue;
mod session;
mod settings;
mod subagent;
mod title;

pub use account::{Account, AgentAccount, LimitWindow, SessionAccount, UsageLimit};
pub use cache::{CACHE_WORTH_TELLING_TOKENS, CacheRequest, CacheState, CacheTtl, PromptCache};
pub use context::{ContextPart, ContextPartKind, ContextUsage};
pub use draft::{Draft, NewDraft};
pub use effort::Effort;
pub use files::{FileContents, WorktreeFiles};
pub use hub_settings::{HubSettings, HubSettingsChange};
pub use model_name::{ModelName, ModelNameError};
pub use permission_mode::PermissionMode;
pub use prompt_line::PromptLine;
pub use pull_request::{
    Checks, Local, MergeState, PrState, PullRequest, PullRequestFound, QueueState, Review, Standing,
};
pub use queue::QueuedPrompt;
pub use session::{AgentKind, Session, SessionStatus};
pub use settings::{DEFAULT_EFFORT, DEFAULT_MODEL, SessionSettings, SessionSettingsChange};
pub use subagent::{Call, CallResult, RecentCall, RecentWork, Said, Subagent, SubagentStatus};
pub use title::{Title, TitleError, TitleSource};

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
    /// An account an agent runs under, as its adapter names it. It's opaque, says which kind of agent it's for, and
    /// is the same for every agent under the same login, so the hub can keep what's the account's, such as its
    /// limits, in one place.
    AccountId,
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
    /// Chosen by whoever has the file first, as a UUID: a client before it adds the file, so it can show it at once
    /// and adding it again can't add it twice, or the worker for an image a tool sent back. Unique within its session.
    AttachmentId,
    /// Chosen by a client, so that retrying a request that creates something can't create it twice.
    ClientRequestId,
    /// The agent's own ID for a message it's sending.
    MessageId,
    /// The agent's own ID for one use of a tool.
    ToolUseId,
    /// The agent's own ID for work it runs in the background.
    TaskId,
    /// A commit, by the full object ID git gives it. It holds any string, as every ID here does, so it's the worker
    /// reading it that refuses one that isn't a full object ID.
    CommitId,
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
    /// An event's place among the events from the same agent or subagent in its session, so a client can follow one of
    /// them apart from the rest. The first is 1, and there are no gaps. A hub that doesn't record it sends none, which
    /// reads as 0.
    #[derive(Default)]
    Place,
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
    /// A hub has one record of its settings.
    HubSettings,
    Account {
        id: AccountId,
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
    /// `None` from a hub from before it kept any.
    #[serde(default)]
    pub hub_settings: Option<HubSettings>,
    /// Empty from a hub from before it kept them.
    #[serde(default)]
    pub accounts: Vec<Account>,
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
    /// Its worktree's directory, the branch it started on, and `CONDUCTOR_WORKSPACE_NAME`. Unique in its
    /// repository, and never changes, because renaming a worktree under a running agent would break it.
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
    /// When the person last did something in it, by the hub's clock: made it, made a chat in it, or sent one of its
    /// agents something, as that chat's `user_active_at_ms` says, which moves in the same commit. `None` from a hub
    /// that doesn't keep it. [`Workspace::last_active_at_ms`] says when it was last worked in from it.
    #[serde(default)]
    pub user_active_at_ms: Option<i64>,
    pub setup: SetupStatus,
    /// The first of ten ports set aside for this workspace, so workspaces don't collide.
    pub port_base: u16,
    #[serde(default)]
    pub rev: Rev,
    /// What it's called on screen. `None` until its first prompt or a rename.
    #[serde(default, deserialize_with = "Title::deserialize_optional")]
    pub title: Option<Title>,
    #[serde(default)]
    pub title_source: TitleSource,
    /// Its branch's pull request, as the hub last found it, kept across restarts so every client can show it at once.
    /// `None` until the hub has looked, and from a hub too old to look.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "pull_request::deserialize_found")]
    pub pull_request: Option<PullRequestFound>,
}

impl Workspace {
    /// Its pull request, if the hub has found one.
    pub fn known_pull_request(&self) -> Option<&PullRequest> {
        self.pull_request.as_ref().and_then(|found| found.pull_request.as_ref())
    }

    /// What to call it on screen: its title, or its name until it has one.
    pub fn label(&self) -> &str {
        self.title.as_deref().unwrap_or(&self.name)
    }

    /// When the person last worked in it, by the hub's clock, as `user_active_at_ms` says. Only what they did counts,
    /// so it doesn't move as its agents work or reply. A hub too old to keep that says only when it was made.
    pub fn last_active_at_ms(&self) -> i64 {
        self.user_active_at_ms.map_or(self.created_at_ms, |at| at.max(self.created_at_ms))
    }
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
    /// The commit the diff is taken from, which holds each file as it was before the change: the merge base with the
    /// workspace's base ref, or HEAD's commit when there's no base ref or git finds no merge base with it. Every diff
    /// names it, since with no commit at HEAD there's no diff to take. `None` only from a worker older than this,
    /// which took a diff with no merge base from HEAD without naming its commit.
    pub base_commit: Option<CommitId>,
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
