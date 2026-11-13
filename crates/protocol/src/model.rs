//! Records the hub owns and serves.

use serde::{Deserialize, Serialize};

mod account;
mod agent;
mod branches;
mod cache;
mod compaction_line;
mod context;
mod counters;
mod draft;
mod effort;
mod files;
mod git;
mod github_account;
mod hub_settings;
mod model_name;
mod permission_mode;
mod prompt_line;
mod pull_request;
mod queue;
mod record;
mod session;
mod settings;
mod subagent;
mod title;
mod wait;
mod worktrees;

pub use account::{
    Account, AgentAccount, AgentLogin, HeldLogin, LimitWindow, LoginToken, ModelLabel, SessionAccount, UsageLimit,
};
pub use agent::{AgentDescription, AgentFeature, LoginDescription, ModelChoice, NotOffered};
pub use branches::{Branch, Branches, Head, Upstream};
pub use cache::{CACHE_WORTH_TELLING_TOKENS, CacheRequest, CacheState, CacheTtl, PromptCache};
pub use compaction_line::CompactionLine;
pub use context::{ContextPart, ContextPartKind, ContextUsage, MaxSetBy};
pub use counters::{Place, Rev, RunId, Seq};
pub use draft::{
    AnswerDraft, AnswerSoFar, CommentDraft, Draft, DraftToKeep, NewWorkspaceDraft, NewWorkspaceDraftToKeep,
};
pub use effort::Effort;
pub use files::{FileContents, FileOpened, FilePage, FilePages, OpenedText, PageLine, WorktreeFiles};
pub use git::{GitDiff, GitFile, GitStatus};
pub use github_account::{
    FoundGithubLogin, GithubAccount, GithubLogin, GithubLoginKind, GithubToken, GithubUser, GithubUserId,
    HeldGithubLogin,
};
pub use hub_settings::{GithubDefault, GithubSettings, HubSettings, HubSettingsChange};
pub use model_name::{ModelName, ModelNameError};
pub use permission_mode::PermissionMode;
pub use prompt_line::{PromptLine, SettledAt, Settlement};
pub use pull_request::{
    Checks, LeftQueue, Local, MergeState, PrState, PullRequest, PullRequestFound, QueueState, Review, Standing,
};
pub use queue::QueuedPrompt;
pub use record::{Record, RecordKey, Records};
pub use session::{AgentKind, OpenApproval, Session, SessionStatus};
pub use settings::{SessionSettings, SessionSettingsChange};
pub use subagent::{Call, CallResult, RecentCall, RecentWork, Said, Subagent, SubagentStatus};
pub use title::{Title, TitleError, TitleSource};
pub use wait::{Wait, WaitEnd, WaitEnded, WaitUntil};
pub use worktrees::{CHILDREN, ChildStatus, ChildWorktree, MadeBy, WorkName, WorkNameError};

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

/// Declares token types: text that's all it takes to act as whoever it's for, so it's kept out of logs. Its `Debug`
/// says only that it's there, it has no `Display`, and a request or a reply that carries one can be logged whole. It's
/// a plain string on the wire and in storage. A module that owns one declares it with `crate::model::tokens!`.
macro_rules! tokens {
    ($($(#[$doc:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$doc])*
        #[derive(::serde::Serialize, ::serde::Deserialize, Clone, PartialEq, Eq, Hash)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(token: impl Into<String>) -> Self {
                Self(token.into())
            }

            /// The token itself, for handing on or keeping, and nothing else.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(concat!(stringify!($name), "(..)"))
            }
        }
    )+};
}

pub(crate) use tokens;

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
    /// One of a hub's providers, by the name the hub has it under, such as `local`, which each workspace records.
    ProviderName,
    /// A hub's own ID, made once and kept, so a provider adapter serving several hubs on one host works only with
    /// the instances it made for the one that's asking.
    HubId,
    /// The Rust target a worker binary is built for, such as `x86_64-unknown-linux-gnu`.
    WorkerTarget,
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
    /// A wait on a workspace's pull request, chosen by the hub. The MCP task its agent follows it by has the same ID.
    WaitId,
    /// A commit, by the full object ID git gives it. It holds any string, as every ID here does, so it's the worker
    /// reading it that refuses one that isn't a full object ID.
    CommitId,
    /// A branch, by its name without `refs/heads/`, such as `euston`, as git spells it.
    BranchName,
    /// A remote of a repository, such as `origin`, as git's config names it.
    RemoteName,
    /// The hub's ID for one of a workspace's other worktrees, as a UUID, since a path names a new one once the last
    /// there is removed.
    ChildWorktreeId,
    /// A worker's own name for something it asks its hub to make, as a UUID or the agent's ID for the tool call that
    /// asked, so a hub that hears the ask again, as each new hub does what the last didn't answer, makes it once.
    /// Unique within the session that asked, as a tool use's ID is.
    AskKey,
    /// A GitHub account the hub holds, as a UUID the hub chose, since one GitHub user can be connected more than once,
    /// as with a token beside a sign-in.
    GithubAccountId,
    /// A GitHub user's name, such as `ada`, as GitHub spells it. GitHub's API calls it the user's `login`, a word that
    /// here means a credential. The user can change it, and GitHub compares it without case, so a user is told apart
    /// by its [`GithubUserId`].
    GithubUsername,
);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    pub id: RepoId,
    pub name: String,
    pub path: String,
    pub created_at_ms: i64,
    #[serde(default)]
    pub rev: Rev,
}

impl ProviderName {
    /// What a hub calls the provider that runs workers on its own machine.
    pub const LOCAL: &'static str = "local";

    pub fn is_local(&self) -> bool {
        self.as_str() == Self::LOCAL
    }
}

/// How many ports are set aside for each workspace, from its [`Workspace::port_base`].
pub const PORTS_PER_WORKSPACE: u16 = 10;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Creating,
    Ready,
    /// On its way to `Archived`, from the moment it's asked, while the hub checks nothing would be lost, runs the
    /// archive script, stops its agents and removes its worktree. Back to `Ready` if any of that refuses or fails.
    /// An older hub says `Ready` throughout.
    Archiving,
    Archived,
    /// On its way back to `Ready`, from the moment it's asked, while its worker makes its worktree again. Back to
    /// `Archived` if that fails. An older hub says `Archived` throughout.
    Restoring,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub repo_id: RepoId,
    /// Its worktree's directory, the folder its branches go in, and `CONDUCTOR_WORKSPACE_NAME`. Unique in its
    /// repository, and never changes once it's ready, because renaming a worktree under a running agent would break
    /// it. While it's being made it can take another, if git already has a directory, a branch or a folder of
    /// branches by the one it has.
    pub name: String,
    /// The branch it started on, `euston/start` for `euston`, or `euston` itself for one made before its branches went in
    /// a folder. Once it's archived, the branch it had checked out, which restoring checks out again.
    pub branch: BranchName,
    /// What the branch was created from, such as `origin/main`. Diffs are taken against this.
    pub base_ref: Option<String>,
    /// The worktree's path on the worker's machine. Empty until the worker has created it.
    pub path: Option<String>,
    pub status: WorkspaceStatus,
    pub provider: ProviderName,
    pub instance_id: InstanceId,
    pub created_at_ms: i64,
    pub archived_at_ms: Option<i64>,
    /// When the person last did something in it, by the hub's clock: made it, made a chat in it, or sent one of its
    /// agents something, as that chat's `user_active_at_ms` says, which moves in the same commit. `None` from a hub
    /// that doesn't keep it. [`Workspace::last_active_at_ms`] says when it was last worked in from it.
    #[serde(default)]
    pub user_active_at_ms: Option<i64>,
    pub setup: SetupStatus,
    /// The first of the [`PORTS_PER_WORKSPACE`] ports set aside for this workspace, so workspaces don't collide.
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
    /// The comment being written on its diff and not saved, as [`CommentDraft`] says. `None` from a hub that doesn't
    /// keep it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment_draft: Option<CommentDraft>,
    /// Its worktree's branches, as its worker last read them, kept across restarts. `None` until a worker has watched
    /// it, and from a hub too old to watch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branches: Option<Branches>,
    /// The branch the worktree had checked out when the hub last looked for its pull request, which is the branch
    /// `pull_request` is. `None` before the hub kept it, and when HEAD was detached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request_branch: Option<BranchName>,
    /// Which build its worker is, while that's another than the hub's: one an earlier hub left running, as after an
    /// update, which the hub carries on with until nothing in the workspace is at work, and then replaces with its own.
    /// `None` while its worker, or the next one, is the hub's own build, since every worker a hub starts is, and from a
    /// hub too old to say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_build: Option<String>,
}

impl Workspace {
    /// Its pull request, if the hub has found one.
    pub fn known_pull_request(&self) -> Option<&PullRequest> {
        self.pull_request.as_ref().and_then(|found| found.pull_request.as_ref())
    }

    /// Whether what the hub last found of its pull request was for another branch than the one checked out, so it's
    /// no longer what the workspace's branch has, whatever it says. Not when nothing's been found yet, nor when it's
    /// not known what's checked out, or HEAD is detached, which has no pull request. Nor when none was found on no
    /// branch, as where `gh` can't work at all, such as without a login, which checking out another doesn't change.
    pub fn pull_request_found_on_another_branch(&self) -> bool {
        let Some(Head::Branch { name }) = self.branches.as_ref().map(|branches| &branches.head) else { return false };
        let found_on_a_branch = self.pull_request.is_some() && self.pull_request_branch.is_some();
        let found = self.known_pull_request().is_some() || found_on_a_branch;
        found && self.pull_request_branch.as_ref() != Some(name)
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
