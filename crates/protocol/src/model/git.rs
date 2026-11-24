//! What a worktree's git says of its files: which have changed, and the diff of those changes.

use serde::{Deserialize, Serialize};

use super::{BranchName, CommitId};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct GitStatus {
    /// The branch checked out, or `None` on a detached HEAD.
    pub branch: Option<BranchName>,
    /// The commit checked out, or `None` on a branch with no commit yet.
    pub head: Option<CommitId>,
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GitDiff {
    /// The commit the diff is taken from, which holds each file as it was before the change: the merge base with the
    /// workspace's base ref, or HEAD's commit when there's no base ref or git finds no merge base with it. Every diff
    /// names it, since with no commit at HEAD there's no diff to take.
    pub base_commit: CommitId,
    /// git's diff of what it tracks, then each untracked file's against nothing. So a file taken out of git's index and
    /// left in the worktree is in it twice, its deletion first, then the untracked file.
    pub patch: String,
    /// True when untracked files were left out to keep the response bounded.
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What would go with the worktree: changes not yet committed, and commits on no branch, which a branch would
    /// otherwise outlive the worktree to keep.
    #[test]
    fn removing_a_worktree_would_lose_what_isnt_committed_and_commits_no_branch_holds() {
        let changed = GitFile { path: "a.txt".into(), orig_path: None, status: ".M".into() };
        let on_a_branch = GitStatus { branch: Some(BranchName::from("alder")), ..GitStatus::default() };
        let on_no_branch = GitStatus { unreferenced_commits: true, ..GitStatus::default() };
        let cases = [
            ("nothing", on_a_branch.clone(), None),
            ("an uncommitted change", GitStatus { files: vec![changed], ..on_a_branch }, Some("uncommitted")),
            ("commits on no branch", on_no_branch, Some("detached")),
            ("a detached HEAD a branch holds", GitStatus::default(), None),
        ];

        for (name, status, says) in cases {
            let loss = status.loss_if_removed();

            let said = loss.as_deref().map(|loss| ["uncommitted", "detached"].into_iter().find(|w| loss.contains(w)));
            assert_eq!(said, says.map(Some), "{name}: {loss:?}");
        }
    }
}
