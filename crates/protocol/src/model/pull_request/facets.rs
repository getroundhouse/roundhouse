//! A pull request as the hub last read it from GitHub, in facets: its summary, which every read of it takes, and the
//! lists that cost more to read, each read on its own and kept with when it was read, as
//! `docs/principles/outside-systems.md` says. A chip's has only its summary, and the bar's and a wait's have them all.

use serde::{Deserialize, Serialize};

use super::super::references::About;
use super::super::{BranchName, CommitId, GithubRepositoryName, PullRequestNumber};
use super::status::{LeftQueue, MergeState, PrState, QueueState, Review};

/// A pull request on GitHub, in the facets the hub has read of it. A facet that's `None` hasn't been read, as for a
/// chip's, which only needs its summary, or since the hub's cache was emptied, and nothing is to be taken from it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PullRequest {
    /// The repository it's in, as GitHub names it.
    pub repository: GithubRepositoryName,
    pub number: PullRequestNumber,
    pub summary: Read<Summary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<Read<History>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<Read<Stack>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<Read<Checks>>,
}

/// What a facet said, and when the read of it began, in milliseconds since the epoch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Read<T> {
    pub said: T,
    pub read_at_ms: i64,
}

/// What GitHub says of a pull request in its plain fields, which cost no more than asking for it: what a chip's card
/// says of it, where it stands, the commits it's at, and its newest commit's checks together.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub about: About,
    pub state: PrState,
    pub is_draft: bool,
    #[serde(default)]
    pub review: Review,
    /// Where it is in the merge queue, or `None` when it isn't in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<QueueState>,
    #[serde(default)]
    pub merge_state: MergeState,
    pub mergeable: Mergeability,
    /// The branch it merges into.
    pub base_branch: BranchName,
    pub head: CommitId,
    /// Where its base branch is.
    pub base: CommitId,
    /// Lines its changes add and remove.
    pub added: u32,
    pub removed: u32,
    /// Where its newest commit's checks stand together, as GitHub rolls them up, required or not, or `None` when it has
    /// none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollup: Option<ChecksRollup>,
}

/// When a pull request last had changes requested, and last left the merge queue without merging, each the newest of
/// a list GitHub keeps.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct History {
    /// When a review last asked for changes, unless it's been dismissed. Apart from the summary's `review`, which
    /// GitHub keeps at `ChangesRequested` through every later review that asks again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes_requested_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_queue: Option<LeftQueue>,
}

/// Where a pull request is in its stack on GitHub, if it's in one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Stack {
    /// Where it merges in the end: its stack's trunk, or the branch it merges into when it's in none.
    pub destination: BranchName,
    /// Those open under it, lowest first.
    pub dependencies: Vec<PullRequestNumber>,
    /// Whether the one just under it is an ancestor of its head, or `None` when that couldn't be told, which never
    /// means it is.
    pub linear: Option<bool>,
}

/// The checks reported on the commit GitHub gives a pull request's checks on, which is its head, or the merge queue's
/// test merge of it, each with whether its base branch's rules require it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Checks {
    /// The commit they ran on, or `None` when GitHub has none for it, so none are known.
    pub revision: Option<CommitId>,
    /// Whether the rules of its base branch could be read, without which an empty list isn't success.
    pub requirements_known: bool,
    pub checks: Vec<ObservedCheck>,
}

/// One check on a pull request's commit.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ObservedCheck {
    pub name: String,
    /// Tells providers and workflows with the same check name apart.
    pub source: String,
    pub url: Option<String>,
    pub state: CheckState,
    pub required: bool,
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

/// GitHub's `mergeable`: whether its branch merges into its base without conflicts.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mergeability {
    Mergeable,
    Conflicting,
    #[serde(other)]
    Unknown,
}

/// GitHub's word for where a commit's checks stand together.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChecksRollup {
    Passed,
    Failed,
    /// Queued, running, or waiting to be reported.
    Running,
    /// One this version doesn't know.
    #[serde(other)]
    Unknown,
}

/// How many of a pull request's checks passed, failed and haven't finished.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CheckCounts {
    pub passed: u32,
    pub failed: u32,
    /// Queued or running, or a state this version doesn't know.
    pub pending: u32,
}

impl Checks {
    /// How many passed, failed and haven't finished.
    pub fn counts(&self) -> CheckCounts {
        let mut counts = CheckCounts::default();
        for check in &self.checks {
            match check.state {
                CheckState::Passed => counts.passed += 1,
                CheckState::Failed => counts.failed += 1,
                CheckState::Pending | CheckState::Unknown => counts.pending += 1,
            }
        }
        counts
    }
}

impl PullRequest {
    /// Its summary, as GitHub last said it.
    pub fn said(&self) -> &Summary {
        &self.summary.said
    }

    /// What its history said, if it's been read.
    pub fn history_said(&self) -> Option<&History> {
        self.history.as_ref().map(|read| &read.said)
    }

    /// Where it is in its stack, if that's been read.
    pub fn stack_said(&self) -> Option<&Stack> {
        self.stack.as_ref().map(|read| &read.said)
    }

    /// Its checks, if they've been read.
    pub fn checks_said(&self) -> Option<&Checks> {
        self.checks.as_ref().map(|read| &read.said)
    }

    /// Whether `other` says the same of it in every facet, read or not, however far apart the two were read, since
    /// each read is newer than the last.
    pub fn says_the_same(&self, other: &PullRequest) -> bool {
        (self.repository == other.repository && self.number == other.number && self.said() == other.said())
            && self.history_said() == other.history_said()
            && self.stack_said() == other.stack_said()
            && self.checks_said() == other.checks_said()
    }
}
