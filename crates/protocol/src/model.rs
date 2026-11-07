//! Records the hub owns and serves.

use serde::{Deserialize, Serialize};

pub type RepoId = String;
pub type WorkspaceId = String;
pub type SessionId = String;
/// Position in a session's event log. The first event is 1, and there are no gaps.
pub type Seq = u64;

/// A `seq` as a position in a list of events.
#[expect(
    clippy::cast_possible_truncation,
    reason = "only 64-bit targets are supported, where this can't lose anything"
)]
pub const fn seq_index(seq: Seq) -> usize {
    seq as usize
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    pub id: RepoId,
    pub name: String,
    pub path: String,
    pub created_at_ms: i64,
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
    pub instance_id: String,
    pub created_at_ms: i64,
    pub archived_at_ms: Option<i64>,
    pub setup: SetupStatus,
    /// The first of ten ports set aside for this workspace, so workspaces don't collide.
    pub port_base: u16,
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
    #[serde(other)]
    Unknown,
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
    pub last_seq: Seq,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// The start of the first prompt. `None` until there's been one.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub closed_at_ms: Option<i64>,
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
    pub id: String,
    pub workspace_id: WorkspaceId,
    pub path: String,
    pub side: DiffSide,
    pub line: u32,
    pub text: String,
    pub created_at_ms: i64,
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
