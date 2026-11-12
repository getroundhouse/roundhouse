//! What a worktree's git says of its files: which have changed, and the diff of those changes.

use serde::{Deserialize, Serialize};

use super::CommitId;

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
