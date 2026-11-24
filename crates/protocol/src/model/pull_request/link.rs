//! Which pull request is a workspace's, as its record keeps it. What GitHub says of it is kept apart, in the hub's
//! cache, and sent to a client that shows it.

use serde::{Deserialize, Serialize};

use super::super::{GithubRepositoryName, IssueNumber};

/// Which pull request the hub found the last time it looked for a branch's. Written as the link, or `null`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub struct PullRequestLinked {
    /// `None` when the branch has none.
    pub link: Option<PullRequestLink>,
}

/// A workspace's pull request, by where it is, which GitHub alone can't always say again, as once the workspace is
/// archived, or its branch is deleted or its name used again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PullRequestLink {
    /// The repository it merges into, which for a clone of a fork is the one the fork was made from.
    pub repository: GithubRepositoryName,
    pub number: IssueNumber,
    pub url: String,
    /// How many files the worktree had changed and not committed when the hub last read the pull request. A hub keeps
    /// it in the observation `WorkspacePullRequest`, and fills this from it for a client of a version from before it,
    /// 0 where it holds none.
    #[serde(default)]
    pub uncommitted: u32,
}
