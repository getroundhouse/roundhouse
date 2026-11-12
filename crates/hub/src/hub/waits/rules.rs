//! When a wait ends: written once, from the wait and the pull request as a read finds it, the same as it starts and
//! after.

use protocol::model::{MergeState, PrState, PullRequest, QueueState, Wait, WaitEnd, WaitUntil};

use crate::store::Seen;

#[cfg(test)]
mod tests;

/// What GitHub has recorded of `pr` that a wait started from it would end on if it came again.
pub(super) fn seen_in(pr: &PullRequest) -> Seen {
    Seen {
        left_queue_at_ms: pr.left_queue.as_ref().map(|left| left.at_ms),
        changes_requested_at_ms: pr.changes_requested_at_ms,
    }
}

/// How `wait` ends, if the pull request as read now, `now`, ends it. What GitHub records ends it only when it's newer
/// than what it had recorded as the wait started, `seen`, both by GitHub's clock, so what came before ends nothing.
///
/// - Done comes first: merged or closed, or the branch's pull request isn't the one it waits on any more.
/// - Then the merge queue, which merges it whatever else is true. So a queued pull request that reads as conflicting
///   for a moment, as a stack's upper one does while the queue merges both, waits on. The queue saying it can't merge
///   it ends the wait, and so does it leaving the queue without merging, as GitHub records it, so even a stay too short
///   for any read to see counts.
/// - Then what only the agent can deal with: a review asking for changes since the wait started, since GitHub keeps
///   its decision at that through later reviews that ask again. Then, for a wait until it's done, a draft, which never
///   merges. Then, once the pull request has the agent's commits, since until then GitHub is talking about one they
///   replace: conflicts, or a failed check that holds the merge back.
/// - A wait until the checks ends on any failed check, and once they've run and none failed.
///
/// What's only in the worktree, such as changes not committed, ends nothing, since it's what the agent is doing.
pub(super) fn ended(wait: &Wait, seen: Seen, now: Option<&PullRequest>) -> Option<WaitEnd> {
    let Some(pr) = now.filter(|pr| pr.number == wait.number) else { return Some(WaitEnd::NoPullRequest) };
    match pr.state {
        PrState::Merged => return Some(WaitEnd::Merged),
        PrState::Closed => return Some(WaitEnd::Closed),
        // Whatever GitHub said isn't known here, so it waits for a read that says.
        PrState::Unknown => return None,
        PrState::Open => {}
    }
    let now_seen = seen_in(pr);
    let newer = |at_ms: Option<i64>, seen: Option<i64>| at_ms.is_some_and(|at_ms| seen.is_none_or(|seen| at_ms > seen));
    match pr.queue {
        Some(QueueState::Unmergeable) => return Some(WaitEnd::LeftQueue),
        Some(
            QueueState::AwaitingChecks
            | QueueState::Locked
            | QueueState::Mergeable
            | QueueState::Queued
            | QueueState::Unknown,
        ) => return None,
        None if newer(now_seen.left_queue_at_ms, seen.left_queue_at_ms) => return Some(WaitEnd::LeftQueue),
        None => {}
    }
    if newer(now_seen.changes_requested_at_ms, seen.changes_requested_at_ms) {
        return Some(WaitEnd::ChangesRequested);
    }
    let until_checks = wait.until == WaitUntil::Checks;
    if pr.is_draft && !until_checks {
        return Some(WaitEnd::Draft);
    }
    if pr.local.ahead > 0 {
        return None;
    }
    let held_back = match pr.merge_state {
        MergeState::Dirty => return Some(WaitEnd::Conflicts),
        MergeState::Behind | MergeState::Blocked | MergeState::Draft => true,
        // GitHub can merge it all the same, or is still working out whether, which a later read says.
        MergeState::Clean
        | MergeState::HasHooks
        | MergeState::Unstable
        | MergeState::Computing
        | MergeState::Unknown => false,
    };
    if pr.checks.failed > 0 && (until_checks || held_back) {
        return Some(WaitEnd::ChecksFailed);
    }
    // Checks that haven't started yet, as just after a push, are none passed and none running, which isn't done.
    let checks_done = pr.checks.pending == 0 && pr.checks.passed > 0;
    (until_checks && checks_done).then_some(WaitEnd::ChecksPassed)
}
