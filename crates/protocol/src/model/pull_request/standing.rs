use super::facets::{CheckCounts, PullRequest};
use super::phase::Phase;
use super::status::{MergeState, OwnPullRequest, QueueState, Review};

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
    ChecksFailed(CheckCounts),
    ChecksRunning(CheckCounts),
    /// Its checks haven't been read yet, as just after a chip's read found it, so what's in the way can't be told.
    Reading,
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
    pub fn of(own: Option<&OwnPullRequest>) -> Self {
        let Some(OwnPullRequest { pull_request: pr, local }) = own else { return Standing::NoPullRequest };
        if let Some(done_or_queued) = Self::done_or_queued(pr) {
            return done_or_queued;
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

    /// Where it stands by what GitHub says alone, leaving out the worktree, which a wait on it goes by.
    pub fn on_github_alone(pr: &PullRequest) -> Self {
        Self::done_or_queued(pr).unwrap_or_else(|| Self::on_github(pr))
    }

    fn done_or_queued(pr: &PullRequest) -> Option<Self> {
        match pr.phase() {
            Phase::Merged => Some(Standing::Merged),
            Phase::Closed => Some(Standing::Closed),
            Phase::Queued(queue) => Some(Standing::Queued(queue)),
            Phase::Unknown => Some(Standing::Unknown),
            Phase::Open | Phase::Draft => None,
        }
    }

    fn on_github(pr: &PullRequest) -> Self {
        let said = pr.said();
        if said.merge_state == MergeState::Dirty {
            return Standing::Conflicts;
        }
        if said.review == Review::ChangesRequested {
            return Standing::ChangesRequested;
        }
        if pr.phase() == Phase::Draft {
            return Standing::Draft;
        }
        let Some(checks) = &pr.checks else { return Standing::Reading };
        let counts = checks.said.counts();
        if counts.failed > 0 {
            return Standing::ChecksFailed(counts);
        }
        if counts.pending > 0 {
            return Standing::ChecksRunning(counts);
        }
        match said.review {
            Review::Required => return Standing::ReviewRequired,
            Review::Unknown => return Standing::Unknown,
            Review::Approved | Review::ChangesRequested | Review::NotRequired => {}
        }
        match said.merge_state {
            MergeState::Behind | MergeState::Blocked => Standing::Blocked,
            MergeState::Computing => Standing::Checking,
            MergeState::Unknown => Standing::Unknown,
            MergeState::Dirty => Standing::Conflicts,
            MergeState::Draft => Standing::Draft,
            MergeState::Clean | MergeState::HasHooks | MergeState::Unstable => Standing::Ready,
        }
    }
}
