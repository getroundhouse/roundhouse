use serde::{Deserialize, Deserializer, Serialize};

use super::facets::PullRequest;

/// What the hub found the last time it looked for a branch's pull request. Written as the pull request, or `null`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub struct PullRequestFound {
    /// `None` when the branch has none.
    pub pull_request: Option<OwnPullRequest>,
}

impl PullRequestFound {
    /// Whether `other` found the same, however far apart the two were read, as [`PullRequest::says_the_same`] says.
    pub fn says_the_same(&self, other: &PullRequestFound) -> bool {
        match (&self.pull_request, &other.pull_request) {
            (Some(one), Some(other)) => one.local == other.local && one.pull_request.says_the_same(&other.pull_request),
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        }
    }
}

/// A workspace's own pull request, and how its worktree stands against it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnPullRequest {
    pub pull_request: PullRequest,
    pub local: Local,
}

/// Reads a field that's there as `Some`, even when it's `null`, so `null` says "looked, and there's none", apart from
/// a missing field, which says "hasn't looked". The field is left out rather than written as `null` when it's `None`.
pub(crate) fn deserialize_looked<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    from: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(from).map(Some)
}

/// GitHub's `state`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrState {
    Open,
    Closed,
    Merged,
    #[serde(other)]
    Unknown,
}

/// GitHub's `mergeStateStatus`: whether the branch can merge, and if not, why.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MergeState {
    /// Behind its base, which the repository needs it to be up to date with.
    Behind,
    /// Held back by the repository's rules, such as a required review or check.
    Blocked,
    Clean,
    /// It conflicts with its base.
    Dirty,
    Draft,
    /// Clean, with hooks to pass before it merges.
    HasHooks,
    /// Clean, apart from checks that aren't required.
    Unstable,
    /// GitHub hasn't worked it out yet, which it does a moment after anything changes. GitHub calls it `UNKNOWN`.
    #[serde(rename = "UNKNOWN")]
    Computing,
    /// A word this version doesn't know, or none at all. Named apart from GitHub's
    /// `UNKNOWN`, so it reads back as itself.
    #[default]
    #[serde(rename = "UNRECOGNISED", other)]
    Unknown,
}

/// GitHub's `reviewDecision`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Review {
    #[serde(rename = "APPROVED")]
    Approved,
    #[serde(rename = "CHANGES_REQUESTED")]
    ChangesRequested,
    #[serde(rename = "REVIEW_REQUIRED")]
    Required,
    /// The repository doesn't ask for one. GitHub leaves it empty.
    #[default]
    #[serde(rename = "")]
    NotRequired,
    #[serde(other)]
    Unknown,
}

/// GitHub's `mergeQueueEntry.state`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QueueState {
    /// Waiting on the checks the queue runs.
    AwaitingChecks,
    /// The queue is held, such as while a merge it started finishes.
    Locked,
    /// Passed, and merges when its turn comes.
    Mergeable,
    Queued,
    /// It can't merge, and leaves the queue.
    Unmergeable,
    #[serde(other)]
    Unknown,
}

/// GitHub's record of a pull request leaving the merge queue without merging.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LeftQueue {
    pub at_ms: i64,
    /// GitHub's word for why, such as `failed_checks`, `merge_conflict` or `manual`, which it doesn't keep to a list.
    pub reason: String,
}

/// The worktree against the pull request's head on GitHub, as of the last fetch.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Local {
    /// Files changed and not committed.
    pub uncommitted: u32,
    /// Commits here that the pull request doesn't have, and that haven't been pushed, since GitHub's word on the
    /// pull request's head can lag a push by a moment.
    pub ahead: u32,
    /// Commits on the pull request that aren't here.
    pub behind: u32,
}
