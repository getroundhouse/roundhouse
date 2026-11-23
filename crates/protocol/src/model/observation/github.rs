//! What the hub's observations of GitHub say: an issue or a pull request, as one read of GitHub found it, and a
//! workspace's own pull request. None holds a time of its own, since when each was read is
//! [`Observed`](super::Observed)'s, so a read that finds the same says the same. Each is made from, and makes, the
//! form a read of GitHub has, [`PullRequest`] and [`Reference`], which hold when each part was read.

use serde::{Deserialize, Serialize};

use crate::model::{
    Checks, CommitId, GithubRepositoryName, History, Issue, IssueNumber, IssueSummary, Local, OwnPullRequest,
    PullRequest, PullRequestNumber, Read, Reference, ReferenceRead, Stack, Summary,
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

impl ReferenceSaid {
    /// What a chip is sent of what a read found, `None` where there's none to be seen. `None` for one of a kind this
    /// build doesn't know.
    pub fn of(found: Option<&Reference>) -> Option<Self> {
        Some(match found {
            None => ReferenceSaid::Missing,
            Some(Reference::Issue(issue)) => ReferenceSaid::Issue(IssueSaid {
                repository: issue.repository.clone(),
                number: issue.number,
                summary: issue.summary.said.clone(),
            }),
            Some(Reference::PullRequest(pull_request)) => ReferenceSaid::PullRequest(PullRequestSaid::of(pull_request)),
            Some(Reference::Unknown) => return None,
        })
    }

    /// It as a read of GitHub says it, as read at `read_at_ms`. `None` for one of a kind this build doesn't know.
    pub fn read(&self, read_at_ms: i64) -> Option<ReferenceRead> {
        let read = |summary| Read { said: summary, read_at_ms };
        let reference = match self {
            ReferenceSaid::Issue(IssueSaid { repository, number, summary }) => Reference::Issue(Issue {
                repository: repository.clone(),
                number: *number,
                summary: read(summary.clone()),
            }),
            ReferenceSaid::PullRequest(said) => Reference::PullRequest(said.pull_request(read_at_ms)),
            ReferenceSaid::Missing => return Some(ReferenceRead::Missing),
            ReferenceSaid::Unknown => return None,
        };
        Some(ReferenceRead::Found { reference: Box::new(reference) })
    }
}

impl PullRequestSaid {
    /// What `pull_request`'s summary says.
    pub fn of(pull_request: &PullRequest) -> Self {
        Self {
            repository: pull_request.repository.clone(),
            number: pull_request.number,
            summary: pull_request.summary.said.clone(),
        }
    }

    /// It as a pull request with its summary alone, read at `read_at_ms`.
    pub fn pull_request(&self, read_at_ms: i64) -> PullRequest {
        PullRequest {
            repository: self.repository.clone(),
            number: self.number,
            summary: Read { said: self.summary.clone(), read_at_ms },
            history: None,
            stack: None,
            checks: None,
        }
    }
}

impl OwnSummary {
    /// What `own`'s summary says, and how the worktree stands against it.
    pub fn of(own: &OwnPullRequest) -> Self {
        Self { pull_request: PullRequestSaid::of(&own.pull_request), local: own.local }
    }

    /// It as a workspace's own pull request, read at `read_at_ms`, with `detail`'s checks, history and stack where
    /// they're of it at the head its summary is at, and otherwise with none of them, since they're of another.
    pub fn own(&self, detail: Option<&PullRequestDetail>, read_at_ms: i64) -> OwnPullRequest {
        let mut pull_request = self.pull_request.pull_request(read_at_ms);
        let of_it = detail.filter(|detail| {
            detail.repository == self.pull_request.repository
                && detail.number == self.pull_request.number
                && detail.head == self.pull_request.summary.head
        });
        if let Some(detail) = of_it {
            pull_request.checks = detail.checks.clone().map(|said| Read { said, read_at_ms });
            pull_request.history = detail.history.clone().map(|said| Read { said, read_at_ms });
            pull_request.stack = detail.stack.clone().map(|said| Read { said, read_at_ms });
        }
        OwnPullRequest { pull_request, local: self.local }
    }
}

impl PullRequestDetail {
    /// What `pull_request` says past its summary, of the head its summary is at.
    pub fn of(pull_request: &PullRequest) -> Self {
        Self {
            repository: pull_request.repository.clone(),
            number: pull_request.number,
            head: pull_request.summary.said.head.clone(),
            checks: pull_request.checks_said().cloned(),
            history: pull_request.history_said().cloned(),
            stack: pull_request.stack_said().cloned(),
        }
    }
}
