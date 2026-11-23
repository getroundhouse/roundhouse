//! What the hub's observations of GitHub say: an issue or a pull request, as one read of GitHub found it. None holds a
//! time of its own, since when each was read is [`Observed`](super::Observed)'s, so a read that finds the same says
//! the same.

use serde::{Deserialize, Serialize};

use crate::model::{GithubRepositoryName, IssueNumber, IssueSummary, PullRequestNumber, Summary};

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
