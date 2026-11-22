use serde::{Deserialize, Serialize};

use crate::model::{PullRequest, WaitEnd};

use super::{PullRequestNumber, PullRequestSelection};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GitHubRepository {
    /// Canonical web URL, including the host, which pins a wait if the worktree's remotes later change.
    pub url: String,
    pub name_with_owner: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PullRequestsSnapshot {
    pub repository: GitHubRepository,
    pub observed_at_ms: i64,
    /// Each with every facet, all read at the same moment.
    pub pull_requests: Vec<PullRequest>,
}

impl PullRequestsSnapshot {
    pub fn get(&self, number: PullRequestNumber) -> Option<&PullRequest> {
        self.pull_requests.iter().find(|pr| pr.number == number)
    }
}

/// When a pull request last had changes requested, and last left the merge queue, as a wait on it started, each by
/// GitHub's own timestamp. Only one later than that ends the wait, so a review or a removal from before the wait
/// started doesn't.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WaitBaseline {
    pub number: PullRequestNumber,
    pub changes_requested_at_ms: Option<i64>,
    pub left_queue_at_ms: Option<i64>,
}

/// A wait's scope and observations. While open the snapshot advances; ending freezes it in the same transaction.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PullRequestWatch {
    pub requested: PullRequestSelection,
    pub snapshot: PullRequestsSnapshot,
    pub baseline: Vec<WaitBaseline>,
    pub deadline_at_ms: Option<i64>,
    /// The failed read after the latest successful observation, if any. Neither replaces that observation.
    pub read_error: Option<String>,
    pub last_attempt_at_ms: i64,
    pub affected: Vec<PullRequestNumber>,
}

impl PullRequestWatch {
    /// The pull requests it watches: those its last reading had in its scope, or those it was asked for where it has
    /// none, as after an update emptied what an older hub kept, until it's read again.
    pub fn numbers(&self) -> Vec<PullRequestNumber> {
        if self.snapshot.pull_requests.is_empty() {
            self.requested.numbers().to_vec()
        } else {
            self.snapshot.pull_requests.iter().map(|row| row.number).collect()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WaitOutcome {
    Reached,
    NeedsAttention,
    TimedOut,
    Unavailable,
    Cancelled,
    #[serde(other)]
    Unknown,
}

/// The immutable terminal result, also returned as structured content by the agent tool.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PullRequestWaitResult {
    pub outcome: WaitOutcome,
    pub reason: WaitEnd,
    pub ended_at_ms: i64,
    pub watch: PullRequestWatch,
}
