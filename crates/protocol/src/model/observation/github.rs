//! What the hub's observations of GitHub say: an issue or a pull request, as one read of GitHub found it, and a
//! workspace's own pull request. None holds a
//! time of its own, since when each was read is [`Observed`](super::Observed)'s, so a read that finds the same says
//! the same.

use serde::{Deserialize, Serialize};

use crate::model::{
    Checks, CommitId, GithubRepositoryName, History, IssueNumber, IssueSummary, Local, PullRequestNumber, Stack,
    Summary,
};

/// What GitHub says of an issue or a pull request a chip names.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReferenceSaid {
    Issue(IssueSaid),
    PullRequest(PullRequestSaid),
    /// There's no such issue or pull request that the hub can see, as in a private repository its token can't read.
    Missing,
    /// One this version doesn't know, which a chip draws as it draws one that isn't known.
    #[serde(other)]
    Unknown,
}

/// An issue's summary, as one read of GitHub found it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct IssueSaid {
    /// The repository it's in, as GitHub names it, which can differ from how it was asked for in case.
    pub repository: GithubRepositoryName,
    pub number: IssueNumber,
    pub summary: IssueSummary,
}

/// A pull request's summary, as one read of GitHub found it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PullRequestSaid {
    /// The repository it's in, as GitHub names it.
    pub repository: GithubRepositoryName,
    pub number: PullRequestNumber,
    pub summary: Summary,
}

/// What the hub found the last time it looked for a workspace's pull request, and how the worktree stands against it.
/// Worked out by the hub from the workspace's record, which says which pull request is its own, from what it last read
/// of GitHub, and from the branches the record keeps.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnPullRequestSaid {
    /// `None` where the workspace's branch has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<OwnSummary>,
}

/// A workspace's own pull request in its summary, and how the worktree stands against it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnSummary {
    pub pull_request: PullRequestSaid,
    pub local: Local,
}

/// What GitHub says of a workspace's own pull request past its summary.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnPullRequestDetail {
    /// `None` where the workspace's branch has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<PullRequestDetail>,
}

/// A pull request's checks, history and stack, as the hub last read them, of the commit its summary was at then. A
/// client puts them with the summary sent at the hub's level only where that summary is of the same pull request at
/// `head`, so a push is never shown with the checks of the commit before it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PullRequestDetail {
    pub repository: GithubRepositoryName,
    pub number: PullRequestNumber,
    /// The commit the summary was at when these were read.
    pub head: CommitId,
    /// Judged by the rules of the branch it merges into, which say which checks are required. `None` where they haven't
    /// been read for `head`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<Checks>,
    /// When a review last asked for changes, and it last left the merge queue. `None` where it hasn't been read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<History>,
    /// Where it is in its stack. `None` where it hasn't been read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<Stack>,
}
