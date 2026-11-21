use super::{
    BranchName, Branches, CommentDraft, Head, InstanceId, ProviderName, PullRequestFound, PullRequestLink,
    PullRequestLinked, RepoId, Rev, Title, TitleSource, WorkspaceId, pull_request,
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
    /// Its branch's pull request, as a hub from before records kept only [`Workspace::pull_request_link`] last found it.
    /// A newer hub never says, and sends what GitHub says of it to a client that shows it instead.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "pull_request::deserialize_looked")]
    pub pull_request: Option<PullRequestFound>,
    /// Which pull request its branch has, as the hub last found, kept across restarts, or that it has none. `None` until
    /// the hub has looked, and from a hub too old to keep it.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "pull_request::deserialize_looked")]
    pub pull_request_link: Option<PullRequestLinked>,
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
    /// Its pull request's link, if the hub has found one.
    pub fn pull_request_linked(&self) -> Option<&PullRequestLink> {
        self.pull_request_link.as_ref().and_then(|linked| linked.link.as_ref())
    }

    /// Whether what the hub last found of its pull request was for another branch than the one checked out, so it's
    /// no longer what the workspace's branch has, whatever it says. Not when nothing's been found yet, nor when it's
    /// not known what's checked out, or HEAD is detached, which has no pull request. Nor when none was found on no
    /// branch, as where GitHub can't be read at all, such as without a login, which checking out another doesn't change.
    pub fn pull_request_found_on_another_branch(&self) -> bool {
        let Some(Head::Branch { name }) = self.branches.as_ref().map(|branches| &branches.head) else { return false };
        let found_on_a_branch = self.pull_request_link.is_some() && self.pull_request_branch.is_some();
        let found = self.pull_request_linked().is_some() || found_on_a_branch;
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
