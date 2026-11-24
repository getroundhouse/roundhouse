use super::{
    BranchName, Branches, CommentDraft, InstanceId, ProviderName, PullRequestLink, PullRequestLinked, RepoId, Rev,
    Title, TitleSource, WorkspaceId, pull_request,
};
use serde::{Deserialize, Serialize};

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
    /// Which pull request its branch has, as the hub last found, kept across restarts, or that it has none. `None` until
    /// the hub has looked, again once GitHub shows no repository where it found none, and from a hub too old to keep it.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "pull_request::deserialize_looked")]
    pub pull_request_link: Option<PullRequestLinked>,
    /// The comment being written on its diff and not saved, as [`CommentDraft`] says. `None` from a hub that doesn't
    /// keep it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment_draft: Option<CommentDraft>,
    /// Its worktree's branches, as its worker last read them. `None` while the hub doesn't know them, as until its
    /// worker next says them after the hub's cache is emptied, and from a hub too old to watch. A hub keeps them as the
    /// observation `WorkspaceBranches`, and fills this from it for a client of a version from before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branches: Option<Branches>,
    /// The branch the worktree had checked out when the hub last looked for its pull request, which is the branch
    /// `pull_request` is. `None` before the hub kept it, when HEAD was detached, and while `pull_request_link` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request_branch: Option<BranchName>,
    /// Which build its worker is, while that's another than the hub's: one an earlier hub left running, as after an
    /// update, which the hub carries on with until nothing in the workspace is at work, and then replaces with its own.
    /// `None` while its worker, or the next one, is the hub's own build, since every worker a hub starts is, from a hub
    /// too old to say, and of a worker too old to name its build. A hub keeps it as the observation
    /// `WorkspaceWorkerBuild`, and fills this from it for a client of a version from before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_build: Option<String>,
}

impl Workspace {
    /// Its pull request's link, if the hub has found one.
    pub fn pull_request_linked(&self) -> Option<&PullRequestLink> {
        self.pull_request_link.as_ref().and_then(|linked| linked.link.as_ref())
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

    /// The order a hub lists workspaces in, and a client keeps them: oldest first, by when each was made, then by ID.
    pub fn oldest_first(&self, other: &Workspace) -> std::cmp::Ordering {
        (self.created_at_ms, &self.id).cmp(&(other.created_at_ms, &other.id))
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
