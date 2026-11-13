use super::{
    BTreeSet, CheckState, MergeState, Mergeability, ObservedPullRequest, PrState, PullRequestNumber,
    PullRequestSelection, PullRequestWatch, PullRequestsSnapshot, QueueState, Wait, WaitBaseline, WaitEnd, WaitEnded,
    WaitUntil,
};

pub(in crate::hub::waits) fn scope(
    snapshot: &PullRequestsSnapshot,
    requested: &PullRequestSelection,
) -> Option<PullRequestsSnapshot> {
    let mut numbers: BTreeSet<_> = requested.numbers().iter().copied().collect();
    for number in requested.numbers() {
        numbers.extend(snapshot.get(*number)?.dependencies.iter().copied());
    }
    let mut pull_requests = Vec::new();
    for number in numbers {
        pull_requests.push(snapshot.get(number)?.clone());
    }
    Some(PullRequestsSnapshot { pull_requests, ..snapshot.clone() })
}

pub(in crate::hub::waits) fn baseline(snapshot: &PullRequestsSnapshot) -> Vec<WaitBaseline> {
    snapshot
        .pull_requests
        .iter()
        .filter_map(|row| {
            Some(WaitBaseline {
                number: PullRequestNumber::try_from(row.pull_request.number).ok()?,
                changes_requested_at_ms: row.pull_request.changes_requested_at_ms,
                left_queue_at_ms: row.pull_request.left_queue.as_ref().map(|left| left.at_ms),
            })
        })
        .collect()
}

pub(in crate::hub::waits) fn observe(wait: &mut Wait, snapshot: &PullRequestsSnapshot, at_ms: i64) {
    let Some(watch) = &mut wait.watch else { return };
    if wait.ended.is_some() || watch.snapshot.repository != snapshot.repository {
        return;
    }
    // A foreground read may omit a lower PR that has left the stack. Only a read covering the prior scope
    // can distinguish an expected merge from a changed dependency; scheduled reads include that history.
    if watch.snapshot.pull_requests.iter().any(|row| {
        PullRequestNumber::try_from(row.pull_request.number).ok().is_none_or(|number| snapshot.get(number).is_none())
    }) {
        return;
    }
    let Some(mut scoped) = scope(snapshot, &watch.requested) else { return };
    let changed = topology(watch, &scoped, snapshot);
    // Keep merged lower entries in the result, and record the current state of removed entries for diagnosis.
    for previous in &watch.snapshot.pull_requests {
        let Ok(number) = PullRequestNumber::try_from(previous.pull_request.number) else { continue };
        if scoped.get(number).is_none()
            && let Some(now) = snapshot.get(number)
        {
            scoped.pull_requests.push(now.clone());
        }
    }
    watch.snapshot = scoped;
    watch.last_attempt_at_ms = at_ms;
    watch.read_error = None;
    if let Some(affected) = changed {
        watch.affected = affected;
        wait.ended = Some(WaitEnded { at_ms, how: WaitEnd::StackChanged });
    } else {
        judge(wait, at_ms);
    }
}

fn topology(
    watch: &PullRequestWatch,
    now: &PullRequestsSnapshot,
    full: &PullRequestsSnapshot,
) -> Option<Vec<PullRequestNumber>> {
    let merged = |number| full.get(number).is_some_and(|row| row.pull_request.state == PrState::Merged);
    let active = |numbers: &[PullRequestNumber]| numbers.iter().copied().filter(|n| !merged(*n)).collect::<Vec<_>>();
    let mut changed = Vec::new();
    for old in &watch.snapshot.pull_requests {
        let Ok(number) = PullRequestNumber::try_from(old.pull_request.number) else { continue };
        if merged(number) {
            continue;
        }
        let same = now.get(number).is_some_and(|row| {
            row.destination == old.destination && active(&row.dependencies) == active(&old.dependencies)
        });
        if !same {
            changed.push(number);
        }
    }
    (!changed.is_empty()).then_some(changed)
}

pub(in crate::hub::waits) fn judge(wait: &mut Wait, at_ms: i64) {
    if wait.ended.is_some() {
        return;
    }
    let Some(watch) = &wait.watch else { return };
    let mut blockers = Vec::new();
    let mut done = true;
    for row in &watch.snapshot.pull_requests {
        let Ok(number) = PullRequestNumber::try_from(row.pull_request.number) else {
            done = false;
            continue;
        };
        let seen = watch.baseline.iter().find(|seen| seen.number == number);
        if let Some(how) = blocker(row, seen, wait.until) {
            blockers.push((number, how));
        }
        done &= satisfied(row, wait.until);
    }
    if let Some((_, how)) = blockers.first() {
        wait.ended = Some(WaitEnded { at_ms, how: *how });
        if let Some(watch) = &mut wait.watch {
            watch.affected = blockers.iter().filter(|(_, reason)| reason == how).map(|(n, _)| *n).collect();
        }
    } else if done && !watch.snapshot.pull_requests.is_empty() {
        let how = if wait.until == WaitUntil::Merged { WaitEnd::Merged } else { WaitEnd::ChecksPassed };
        wait.ended = Some(WaitEnded { at_ms, how });
    }
}

fn blocker(row: &ObservedPullRequest, seen: Option<&WaitBaseline>, until: WaitUntil) -> Option<WaitEnd> {
    let pr = &row.pull_request;
    match pr.state {
        PrState::Merged | PrState::Unknown => return None,
        PrState::Closed => return Some(WaitEnd::Closed),
        PrState::Open => {}
    }
    if pr.queue == Some(QueueState::Unmergeable) {
        return Some(WaitEnd::LeftQueue);
    }
    // A queue can transiently report a conflict while merging the stack; it owns that operation.
    if queued(row) {
        return None;
    }
    let newer = |now: Option<i64>, before: Option<i64>| now.is_some_and(|now| before.is_none_or(|before| now > before));
    if newer(pr.left_queue.as_ref().map(|left| left.at_ms), seen.and_then(|seen| seen.left_queue_at_ms)) {
        return Some(WaitEnd::LeftQueue);
    }
    if pr.merge_state == MergeState::Dirty || row.mergeability == Mergeability::Conflicting {
        return Some(WaitEnd::Conflicts);
    }
    if row.linear == Some(false) || pr.merge_state == MergeState::Behind {
        return Some(WaitEnd::NeedsRebase);
    }
    if newer(pr.changes_requested_at_ms, seen.and_then(|seen| seen.changes_requested_at_ms)) {
        return Some(WaitEnd::ChangesRequested);
    }
    if until == WaitUntil::Merged && pr.is_draft {
        return Some(WaitEnd::Draft);
    }
    let all = until == WaitUntil::AllChecks;
    row.checks
        .iter()
        .any(|check| (all || check.required) && check.state == CheckState::Failed)
        .then_some(WaitEnd::ChecksFailed)
}

fn satisfied(row: &ObservedPullRequest, until: WaitUntil) -> bool {
    let pr = &row.pull_request;
    if pr.state == PrState::Merged {
        return true;
    }
    if pr.state != PrState::Open {
        return false;
    }
    let structural = queued(row) || (row.mergeability == Mergeability::Mergeable && row.linear == Some(true));
    if !structural {
        return false;
    }
    match until {
        WaitUntil::RequiredChecks => {
            row.requirements_known
                && row.checks.iter().filter(|check| check.required).all(|check| check.state == CheckState::Passed)
        }
        WaitUntil::AllChecks => {
            row.checks_known
                && !row.checks.is_empty()
                && row.checks.iter().all(|check| check.state == CheckState::Passed)
        }
        WaitUntil::Merged | WaitUntil::Done | WaitUntil::Checks | WaitUntil::Unknown => false,
    }
}

pub(in crate::hub::waits) fn expire(wait: &mut Wait, at_ms: i64) {
    if wait.ended.is_none()
        && wait.watch.as_ref().is_some_and(|watch| watch.deadline_at_ms.is_some_and(|deadline| deadline <= at_ms))
    {
        wait.ended = Some(WaitEnded { at_ms, how: WaitEnd::TimedOut });
    }
}

fn queued(row: &ObservedPullRequest) -> bool {
    matches!(
        row.pull_request.queue,
        Some(QueueState::AwaitingChecks | QueueState::Locked | QueueState::Mergeable | QueueState::Queued)
    )
}
