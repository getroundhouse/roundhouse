use super::super::BranchName;
use serde::{Deserialize, Deserializer, Serialize};

/// What the hub found the last time it looked for a branch's pull request. Written as the pull request, or `null`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub struct PullRequestFound {
    /// `None` when the branch has none.
    pub pull_request: Option<PullRequest>,
}

/// Reads a field that's there as `Some`, even when it's `null`, so `null` says "looked, and there's none", apart from
/// a missing field, which says "hasn't looked". The field is left out rather than written as `null` when it's `None`.
pub(in super::super) fn deserialize_found<'de, D: Deserializer<'de>>(
    from: D,
) -> Result<Option<PullRequestFound>, D::Error> {
    PullRequestFound::deserialize(from).map(Some)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub state: PrState,
    pub is_draft: bool,
    /// The branch it merges into, such as `main`. `None` from a hub that doesn't say.
    #[serde(default)]
    pub base: Option<BranchName>,
    #[serde(default)]
    pub merge_state: MergeState,
    #[serde(default)]
    pub review: Review,
    #[serde(default)]
    pub checks: Checks,
    /// Where it is in the merge queue, or `None` when it isn't in one.
    #[serde(default)]
    pub queue: Option<QueueState>,
    /// The last time it left the merge queue without merging, as GitHub records it. `None` when it never has, or from
    /// a hub that doesn't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_queue: Option<LeftQueue>,
    /// When a review last asked for changes, as GitHub records it, unless it's been dismissed. Apart from `review`,
    /// which GitHub keeps at `ChangesRequested` through every later review that asks again. `None` when none has, or
    /// from a hub that doesn't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes_requested_at_ms: Option<i64>,
    #[serde(default)]
    pub local: Local,
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
    /// A word this version doesn't know, or none at all, from a hub that doesn't say. Named apart from GitHub's
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

/// How the pull request's checks stand on its newest commit.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Checks {
    pub passed: u32,
    pub failed: u32,
    /// Queued or running.
    pub pending: u32,
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

/// Where a workspace's work stands against its pull request: the first thing in the way of merging it, or that
/// nothing is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    NoPullRequest,
    Merged,
    Closed,
    Queued(QueueState),
    /// This many files changed and not committed.
    Uncommitted(u32),
    /// The worktree and the pull request each have commits the other doesn't.
    Diverged,
    /// The pull request has this many commits that aren't here.
    Behind(u32),
    /// This many commits here aren't on the pull request.
    Unpushed(u32),
    Conflicts,
    ChangesRequested,
    Draft,
    ChecksFailed(Checks),
    ChecksRunning(Checks),
    ReviewRequired,
    /// Held back by the repository's other rules, such as needing to be up to date with its base.
    Blocked,
    /// GitHub is still working out whether it can merge.
    Checking,
    Ready,
    /// GitHub said something this version doesn't know, or the hub didn't say enough to tell, so nothing is said
    /// beyond it being on GitHub.
    Unknown,
}

impl Standing {
    /// Done comes first, then the queue, which merges it whatever else is true. Then what's here and not on
    /// GitHub, since GitHub's answers are about an older commit until it's pushed. Then what GitHub says, the
    /// problems it can't merge with before what it waits on.
    pub fn of(pull_request: Option<&PullRequest>) -> Self {
        let Some(pr) = pull_request else { return Standing::NoPullRequest };
        let local = pr.local;
        match pr.state {
            PrState::Merged => return Standing::Merged,
            PrState::Closed => return Standing::Closed,
            PrState::Unknown => return Standing::Unknown,
            PrState::Open => {}
        }
        if let Some(queue) = pr.queue {
            return Standing::Queued(queue);
        }
        if local.uncommitted > 0 {
            return Standing::Uncommitted(local.uncommitted);
        }
        match (local.ahead, local.behind) {
            (0, 0) => {}
            (0, behind) => return Standing::Behind(behind),
            (ahead, 0) => return Standing::Unpushed(ahead),
            _ => return Standing::Diverged,
        }
        Self::on_github(pr)
    }

    pub(super) fn on_github(pr: &PullRequest) -> Self {
        if pr.merge_state == MergeState::Dirty {
            return Standing::Conflicts;
        }
        if pr.review == Review::ChangesRequested {
            return Standing::ChangesRequested;
        }
        if pr.is_draft || pr.merge_state == MergeState::Draft {
            return Standing::Draft;
        }
        if pr.checks.failed > 0 {
            return Standing::ChecksFailed(pr.checks);
        }
        if pr.checks.pending > 0 {
            return Standing::ChecksRunning(pr.checks);
        }
        match pr.review {
            Review::Required => return Standing::ReviewRequired,
            Review::Unknown => return Standing::Unknown,
            Review::Approved | Review::ChangesRequested | Review::NotRequired => {}
        }
        match pr.merge_state {
            MergeState::Behind | MergeState::Blocked => Standing::Blocked,
            MergeState::Computing => Standing::Checking,
            MergeState::Unknown => Standing::Unknown,
            MergeState::Dirty => Standing::Conflicts,
            MergeState::Draft => Standing::Draft,
            MergeState::Clean | MergeState::HasHooks | MergeState::Unstable => Standing::Ready,
        }
    }
}
