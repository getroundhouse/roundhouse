use super::*;
use protocol::fixtures::{pull_request, pull_request_wait};
use protocol::model::{Checks, History, LeftQueue, ObservedCheck, Stack, Summary};

fn number(n: u64) -> PullRequestNumber {
    PullRequestNumber::try_from(n).unwrap()
}
fn snapshot(wait: &Wait) -> PullRequestsSnapshot {
    wait.watch.as_ref().unwrap().snapshot.clone()
}
fn ended(wait: &Wait) -> Option<WaitEnd> {
    wait.ended.map(|e| e.how)
}
fn summary(pr: &mut PullRequest) -> &mut Summary {
    &mut pr.summary.said
}
fn history(pr: &mut PullRequest) -> &mut History {
    &mut pr.history.as_mut().unwrap().said
}
fn stack(pr: &mut PullRequest) -> &mut Stack {
    &mut pr.stack.as_mut().unwrap().said
}
fn checks_of(pr: &mut PullRequest) -> &mut Checks {
    &mut pr.checks.as_mut().unwrap().said
}
fn checks(pr: &mut PullRequest) -> &mut Vec<ObservedCheck> {
    &mut checks_of(pr).checks
}

#[test]
fn a_lower_conflict_ends_the_wait_while_the_tops_checks_are_running() {
    let mut wait = pull_request_wait("a");
    let watch = wait.watch.as_mut().unwrap();
    stack(&mut watch.snapshot.pull_requests[0]).dependencies = vec![number(3)];
    watch.snapshot.pull_requests.push(pull_request(3));
    let mut now = snapshot(&wait);
    summary(&mut now.pull_requests[1]).mergeable = Mergeability::Conflicting;

    observe(&mut wait, &now, 2);

    assert_eq!(ended(&wait), Some(WaitEnd::Conflicts));
    assert_eq!(wait.watch.unwrap().affected, [number(3)]);
}

#[test]
fn required_checks_ignore_optional_failures_but_all_checks_do_not() {
    let mut wait = pull_request_wait("a");
    let row = &mut wait.watch.as_mut().unwrap().snapshot.pull_requests[0];
    checks(row)[0].state = CheckState::Passed;
    let optional =
        ObservedCheck { name: "optional".into(), required: false, state: CheckState::Failed, ..checks(row)[0].clone() };
    checks(row).push(optional);
    let mut all = Wait { until: WaitUntil::AllChecks, ..wait.clone() };

    judge(&mut wait, 2);
    judge(&mut all, 2);

    assert_eq!(ended(&wait), Some(WaitEnd::ChecksPassed));
    assert_eq!(ended(&all), Some(WaitEnd::ChecksFailed));
}

#[test]
fn unknown_requirements_mergeability_and_missing_checks_never_pass() {
    for case in 0..5 {
        let mut wait = pull_request_wait("a");
        let row = &mut wait.watch.as_mut().unwrap().snapshot.pull_requests[0];
        checks(row)[0].state = CheckState::Passed;
        match case {
            0 => checks_of(row).requirements_known = false,
            1 => summary(row).mergeable = Mergeability::Unknown,
            2 => stack(row).linear = None,
            3 => checks(row)[0].state = CheckState::Pending,
            4 => checks(row)[0].state = CheckState::Unknown,
            _ => unreachable!(),
        }

        judge(&mut wait, 2);

        assert_eq!(ended(&wait), None, "case {case}");
    }
}

#[test]
fn a_push_replaces_the_observation_and_main_advancing_alone_does_not_end_it() {
    let mut wait = pull_request_wait("a");
    let mut now = snapshot(&wait);
    summary(&mut now.pull_requests[0]).head = "new-head".into();
    summary(&mut now.pull_requests[0]).base = "new-main".into();
    checks_of(&mut now.pull_requests[0]).revision = Some("new-head".into());

    observe(&mut wait, &now, 2);
    let mut older = snapshot(&wait);
    older.observed_at_ms -= 1;
    summary(&mut older.pull_requests[0]).head = "head-7".into();
    observe(&mut wait, &older, 3);

    assert_eq!(ended(&wait), None);
    assert_eq!(snapshot(&wait), now, "not one read before it, which finished after it");
}

#[test]
fn all_targets_must_satisfy_the_condition() {
    let mut wait = pull_request_wait("a");
    let watch = wait.watch.as_mut().unwrap();
    watch.requested = PullRequestSelection::try_from(vec![number(7), number(9)]).unwrap();
    watch.snapshot.pull_requests.push(pull_request(9));
    checks(&mut watch.snapshot.pull_requests[0])[0].state = CheckState::Passed;

    judge(&mut wait, 2);
    assert_eq!(ended(&wait), None);
    checks(&mut wait.watch.as_mut().unwrap().snapshot.pull_requests[1])[0].state = CheckState::Passed;
    judge(&mut wait, 3);

    assert_eq!(ended(&wait), Some(WaitEnd::ChecksPassed));
}

#[test]
fn merged_dependencies_can_disappear_but_new_or_reordered_ones_end_the_wait() {
    let mut wait = pull_request_wait("a");
    let watch = wait.watch.as_mut().unwrap();
    stack(&mut watch.snapshot.pull_requests[0]).dependencies = vec![number(3)];
    watch.snapshot.pull_requests.push(pull_request(3));
    let mut merged = snapshot(&wait);
    stack(&mut merged.pull_requests[0]).dependencies.clear();
    summary(&mut merged.pull_requests[1]).state = PrState::Merged;
    let mut after_merge = wait.clone();

    observe(&mut after_merge, &merged, 2);
    assert_eq!(ended(&after_merge), None);
    assert_eq!(snapshot(&after_merge).pull_requests.len(), 2);
    let mut inserted = snapshot(&wait);
    stack(&mut inserted.pull_requests[0]).dependencies.insert(0, number(2));
    inserted.pull_requests.push(pull_request(2));
    observe(&mut wait, &inserted, 2);

    assert_eq!(ended(&wait), Some(WaitEnd::StackChanged));
}

/// A wait an older hub kept, which keeps none of the pull requests it saw, isn't judged done by seeing none, and is
/// judged by the next read as it would have been, by what it saw as it started: a review asked for before it started
/// doesn't end it, and checks that pass do.
#[test]
fn a_wait_kept_without_the_pull_requests_it_saw_is_judged_by_the_next_read() {
    let mut wait = pull_request_wait("a");
    let watch = wait.watch.as_mut().unwrap();
    history(&mut watch.snapshot.pull_requests[0]).changes_requested_at_ms = Some(5);
    checks(&mut watch.snapshot.pull_requests[0])[0].state = CheckState::Passed;
    watch.baseline = baseline(&watch.snapshot);
    let now = snapshot(&wait);
    wait.watch.as_mut().unwrap().snapshot.pull_requests.clear();

    judge(&mut wait, 6);
    assert_eq!(ended(&wait), None, "not done for seeing none");
    observe(&mut wait, &now, 7);

    assert_eq!(ended(&wait), Some(WaitEnd::ChecksPassed));
    assert_eq!(snapshot(&wait).pull_requests.iter().map(|row| row.number).collect::<Vec<_>>(), [number(7)]);
}

#[test]
fn existing_reviews_are_baselined_but_new_requests_wake() {
    let mut wait = pull_request_wait("a");
    let watch = wait.watch.as_mut().unwrap();
    history(&mut watch.snapshot.pull_requests[0]).changes_requested_at_ms = Some(5);
    watch.baseline = baseline(&watch.snapshot);
    judge(&mut wait, 6);
    assert_eq!(ended(&wait), None);
    let mut now = snapshot(&wait);
    history(&mut now.pull_requests[0]).changes_requested_at_ms = Some(7);

    observe(&mut wait, &now, 8);

    assert_eq!(ended(&wait), Some(WaitEnd::ChangesRequested));
}

#[test]
fn queue_authority_precedes_transient_conflicts_but_ejection_wakes() {
    let mut wait = Wait { until: WaitUntil::Merged, ..pull_request_wait("a") };
    let row = &mut wait.watch.as_mut().unwrap().snapshot.pull_requests[0];
    summary(row).mergeable = Mergeability::Conflicting;
    stack(row).linear = Some(false);
    summary(row).queue = Some(QueueState::AwaitingChecks);
    judge(&mut wait, 2);
    assert_eq!(ended(&wait), None);
    let mut now = snapshot(&wait);
    summary(&mut now.pull_requests[0]).queue = None;
    history(&mut now.pull_requests[0]).left_queue = Some(LeftQueue { at_ms: 3, reason: "failed_checks".into() });

    observe(&mut wait, &now, 4);

    assert_eq!(ended(&wait), Some(WaitEnd::LeftQueue));
}

#[test]
fn drafts_allow_checks_waits_and_non_linear_stacks_need_attention() {
    let mut wait = pull_request_wait("a");
    summary(&mut wait.watch.as_mut().unwrap().snapshot.pull_requests[0]).is_draft = true;
    let mut merged = Wait { until: WaitUntil::Merged, ..wait.clone() };
    judge(&mut wait, 2);
    judge(&mut merged, 2);
    assert_eq!(ended(&wait), None);
    assert_eq!(ended(&merged), Some(WaitEnd::Draft));
    stack(&mut wait.watch.as_mut().unwrap().snapshot.pull_requests[0]).linear = Some(false);
    judge(&mut wait, 3);
    assert_eq!(ended(&wait), Some(WaitEnd::NeedsRebase));
}

#[test]
fn deadlines_end_without_a_read_and_terminal_observations_are_frozen() {
    let mut wait = pull_request_wait("a");
    wait.watch.as_mut().unwrap().deadline_at_ms = Some(4);
    expire(&mut wait, 3);
    assert_eq!(ended(&wait), None);
    expire(&mut wait, 4);
    let frozen = wait.clone();
    let mut now = snapshot(&wait);
    checks(&mut now.pull_requests[0])[0].state = CheckState::Passed;

    observe(&mut wait, &now, 5);

    assert_eq!(ended(&wait), Some(WaitEnd::TimedOut));
    assert_eq!(wait, frozen);
}

#[test]
fn a_partial_foreground_read_cannot_turn_a_lower_merge_into_a_stack_change() {
    let mut wait = pull_request_wait("a");
    let watch = wait.watch.as_mut().unwrap();
    stack(&mut watch.snapshot.pull_requests[0]).dependencies = vec![number(3)];
    watch.snapshot.pull_requests.push(pull_request(3));
    let before = wait.clone();
    let mut partial = snapshot(&wait);
    stack(&mut partial.pull_requests[0]).dependencies.clear();
    partial.pull_requests.pop();

    observe(&mut wait, &partial, 2);
    assert_eq!(wait, before);
    let mut merged = pull_request(3);
    merged.summary.said.state = PrState::Merged;
    partial.pull_requests.push(merged);
    observe(&mut wait, &partial, 3);

    assert_eq!(ended(&wait), None);
    assert_eq!(snapshot(&wait).pull_requests[1].said().state, PrState::Merged);
}
