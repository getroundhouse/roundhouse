use super::super::Asked;
use protocol::model::{MergeState, PrState, PullRequest, Workspace, WorkspaceId, WorkspaceStatus};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// The shortest wait between two reads nobody asked for. Soon enough to see a check finish or the merge queue merge it
/// within a couple of minutes.
pub(super) const SOONEST: Duration = Duration::from_mins(2);

/// The longest, for one that has come back the same time after time, such as a branch with no pull request that
/// nobody is working on. A turn there, or showing it, reads it at once anyway.
pub(super) const LATEST: Duration = Duration::from_mins(30);

/// When a workspace's pull request was last read, and how long it goes before the next read nobody asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct Schedule {
    pub(in super::super) read_at: Instant,
    /// Whether the read then found it.
    pub(in super::super) found: bool,
    pub(super) every: Duration,
}

impl Schedule {
    /// Read just now, or found nothing to read, as when a workspace is made from its base, so there's no pull request
    /// yet.
    pub(in super::super) fn starting(now: Instant) -> Self {
        Self { read_at: now, found: false, every: SOONEST }
    }

    /// After a read: back to the soonest when it changed, someone was waiting on it, or it's still moving on GitHub,
    /// and otherwise twice as long as the last wait, up to the latest. So is one that failed, since while GitHub
    /// can't be reached, every workspace would otherwise be tried at the soonest until it can be.
    pub(in super::super) fn after(last: Option<Self>, read_at: Instant, asked: Asked, went: Went) -> Self {
        let every = match went {
            Went::Changed => SOONEST,
            Went::Same(found) if asked != Asked::InTurn || found.is_some_and(moving) => SOONEST,
            Went::Same(_) | Went::Failed => last.map_or(SOONEST * 2, |last| (last.every * 2).min(LATEST)),
        };
        Self { read_at, found: !matches!(went, Went::Failed), every }
    }

    pub(super) fn due_at(self) -> Instant {
        self.read_at + self.every
    }

    /// When one an agent waits on is due, which is always at the soonest.
    pub(super) fn due_at_soonest(self) -> Instant {
        self.read_at + SOONEST
    }
}

/// How a read went.
#[derive(Clone, Copy, Debug)]
pub(in super::super) enum Went<'a> {
    /// It found something other than what was known.
    Changed,
    /// It found what was known.
    Same(Option<&'a PullRequest>),
    /// It couldn't be read, such as while GitHub can't be reached, or without `gh`, a login to it or a worker to read
    /// it with. Showing it reads it at once anyway.
    Failed,
}

/// Whether GitHub is still working on it by itself: checks running, a place in the merge queue, or working out
/// whether it can merge.
fn moving(pr: &PullRequest) -> bool {
    pr.state == PrState::Open
        && (pr.checks.pending > 0 || pr.queue.is_some() || pr.merge_state == MergeState::Computing)
}

/// The ready workspace to read next, once it's due. One not read since the hub started comes first, those with an
/// open pull request first among them, since they're the likeliest to have moved. Then whichever has been due
/// longest. A merged or closed pull request is done, so it's left until someone shows it or an agent's turn ends,
/// unless it was found on another branch than the one checked out, so it isn't the workspace's now, as when the read
/// that should have followed the worktree checking out another failed. One an agent waits on, in `waited`, is due at
/// the soonest, however long it has come back the same, even when it was last read as done, since a read that came
/// before the wait started didn't end it.
pub(in super::super) fn due<'a>(
    workspaces: &'a [Workspace],
    schedules: &HashMap<WorkspaceId, Schedule>,
    waited: &HashSet<WorkspaceId>,
    now: Instant,
) -> Option<&'a Workspace> {
    let state = |w: &Workspace| w.known_pull_request().map(|pr| pr.state);
    let done = |w: &Workspace| {
        matches!(state(w), Some(PrState::Merged | PrState::Closed))
            && !w.pull_request_found_on_another_branch()
            && !waited.contains(&w.id)
    };
    let due_at = |w: &Workspace, schedule: &Schedule| {
        if waited.contains(&w.id) { schedule.due_at_soonest() } else { schedule.due_at() }
    };
    workspaces
        .iter()
        .filter(|w| w.status == WorkspaceStatus::Ready && !done(w))
        .map(|w| (w, schedules.get(&w.id).map(|schedule| due_at(w, schedule))))
        .filter(|(_, due_at)| due_at.is_none_or(|due_at| due_at <= now))
        .min_by_key(|(w, due_at)| (due_at.is_some(), state(w) != Some(PrState::Open), *due_at))
        .map(|(w, _)| w)
}
