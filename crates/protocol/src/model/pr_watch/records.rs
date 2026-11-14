use serde::{Deserialize, Serialize};

use crate::model::{BranchName, CommitId, PullRequest, WaitEnd};

use super::{PullRequestNumber, PullRequestSelection};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GitHubRepository {
    /// Canonical web URL, including the host, which pins a wait if the worktree's remotes later change.
    pub url: String,
    pub name_with_owner: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Passed,
    Failed,
    Pending,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ObservedCheck {
    pub name: String,
    /// Distinguishes providers and workflows with the same check name.
    pub source: String,
    pub url: Option<String>,
    pub state: CheckState,
    pub required: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mergeability {
    Mergeable,
    Conflicting,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ObservedPullRequest {
    pub pull_request: PullRequest,
    pub head: CommitId,
    pub base: CommitId,
    /// Effective destination, which is the stack's trunk for stacked pull requests.
    pub destination: BranchName,
    /// Open dependencies in stack order, lowest first. Entries above this PR never appear here.
    pub dependencies: Vec<PullRequestNumber>,
    pub mergeability: Mergeability,
    /// Whether the immediately lower stack branch is an ancestor of this head. Unknown never means linear.
    pub linear: Option<bool>,
    pub checks: Vec<ObservedCheck>,
    pub checks_revision: Option<CommitId>,
    /// False when effective requirements couldn't be established; an empty list is then not success.
    pub requirements_known: bool,
    /// Complete reported checks for the revision GitHub associates with this PR, including a test merge commit.
    pub checks_known: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PullRequestsSnapshot {
    pub repository: GitHubRepository,
    pub observed_at_ms: i64,
    pub pull_requests: Vec<ObservedPullRequest>,
}

impl PullRequestsSnapshot {
    pub fn get(&self, number: PullRequestNumber) -> Option<&ObservedPullRequest> {
        self.pull_requests.iter().find(|pr| pr.pull_request.number == number.get())
    }
}

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
