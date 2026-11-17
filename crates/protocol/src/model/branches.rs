//! A worktree's branches, as its git holds them: which is checked out, and each branch the worktree has had checked
//! out, with its newest commit and where it's pushed. Reading them is local, so it costs nothing of GitHub's limit,
//! and they change as often as the agent commits, switches branch or pushes, so the worker watches them.

use serde::{Deserialize, Serialize};

use super::{BranchName, CommitId, RemoteName, RepoUrl};

/// What a worktree's git said of its branches when its worker last read them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Branches {
    pub head: Head,
    /// Each branch the worktree has had checked out that still exists, the one checked out now included, by name.
    /// Which it has had is git's own record, the worktree's reflog, so a branch checked out from a terminal counts,
    /// and so does one checked out while nothing was watching. A branch made in another worktree and never checked
    /// out in this one doesn't, and nor does one that's been deleted. What reads them, each branch's own pull request
    /// and what each chat changed, isn't built.
    pub had: Vec<Branch>,
}

impl Branches {
    /// The branch checked out, unless HEAD is detached or its branch has no commit yet.
    pub fn checked_out(&self) -> Option<&Branch> {
        let Head::Branch { name } = &self.head else { return None };
        self.had.iter().find(|branch| &branch.name == name)
    }
}

/// What the worktree has checked out.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Head {
    /// A branch, which is in [`Branches::had`] once it has a commit.
    Branch { name: BranchName },
    /// A commit, on no branch, such as `origin/main` checked out to look at it, or a rebase under way.
    Detached { commit: CommitId },
    /// From a newer peer.
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub name: BranchName,
    /// Its newest commit.
    pub tip: CommitId,
    /// Where it's pushed and pulled, as git's config says. `None` until it's pushed with `-u` or told another way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<Upstream>,
}

/// The branch a local branch is pushed to and pulled from.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Upstream {
    /// `.` for a branch of the same repository.
    pub remote: RemoteName,
    /// Its name there, which needn't be the local branch's, as with `git push -u origin euston:euston/fix-the-header`.
    pub name: BranchName,
    /// The newest commit of it the worktree knows of, from the last push or fetch. `None` before the first, and once
    /// a fetch finds it deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip: Option<CommitId>,
    /// The address of the repository `remote` names, as `git remote get-url` gives it, without any credential it can
    /// hold, so the hub can tell which repository on GitHub the branch is pushed to without asking the worker. A
    /// remote's own, which can be a fork's, rather than the repository's. `None` for `.`, for a remote git has no
    /// address for, and from a worker from before it said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<RepoUrl>,
    /// How far apart the branch and its `tip` are. `None` while there's no `tip`, and from a worker from before it
    /// said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apart: Option<Apart>,
}

/// How far apart a branch and its upstream are, in commits, as of the last push or fetch.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Apart {
    /// The branch's that its upstream doesn't have, which haven't been pushed.
    pub ahead: u32,
    /// The upstream's that the branch doesn't have, such as someone else's push, once it's been fetched.
    pub behind: u32,
}
