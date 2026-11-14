use super::*;
use protocol::fixtures::{observed_pull_request, pull_request_wait};
use protocol::model::{LeftQueue, ObservedCheck};

fn number(n: u64) -> PullRequestNumber {
    PullRequestNumber::try_from(n).unwrap()
}
fn snapshot(wait: &Wait) -> PullRequestsSnapshot {
    wait.watch.as_ref().unwrap().snapshot.clone()
}
fn ended(wait: &Wait) -> Option<WaitEnd> {
    wait.ended.map(|e| e.how)
}

#[test]
fn a_lower_conflict_ends_the_wait_while_the_tops_checks_are_running() {
    let mut wait = pull_request_wait("a");
    let watch = wait.watch.as_mut().unwrap();
    watch.snapshot.pull_requests[0].dependencies = vec![number(3)];
    watch.snapshot.pull_requests.push(observed_pull_request(3));
    let mut now = snapshot(&wait);
    now.pull_requests[1].mergeability = Mergeability::Conflicting;

    observe(&mut wait, &now, 2);

    assert_eq!(ended(&wait), Some(WaitEnd::Conflicts));
    assert_eq!(wait.watch.unwrap().affected, [number(3)]);
}

#[test]
fn required_checks_ignore_optional_failures_but_all_checks_do_not() {
    let mut wait = pull_request_wait("a");
    let row = &mut wait.watch.as_mut().unwrap().snapshot.pull_requests[0];
    row.checks[0].state = CheckState::Passed;
    row.checks.push(ObservedCheck {
        name: "optional".into(),
        required: false,
        state: CheckState::Failed,
        ..row.checks[0].clone()
    });
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
        row.checks[0].state = CheckState::Passed;
        match case {
            0 => row.requirements_known = false,
            1 => row.mergeability = Mergeability::Unknown,
            2 => row.linear = None,
            3 => row.checks[0].state = CheckState::Pending,
            4 => row.checks[0].state = CheckState::Unknown,
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
    now.pull_requests[0].head = "new-head".into();
    now.pull_requests[0].base = "new-main".into();
    now.pull_requests[0].checks_revision = Some("new-head".into());

    observe(&mut wait, &now, 2);

    assert_eq!(ended(&wait), None);
    assert_eq!(snapshot(&wait), now);
}

#[test]
fn all_targets_must_satisfy_the_condition() {
    let mut wait = pull_request_wait("a");
    let watch = wait.watch.as_mut().unwrap();
    watch.requested = PullRequestSelection::try_from(vec![number(7), number(9)]).unwrap();
    watch.snapshot.pull_requests.push(observed_pull_request(9));
    watch.snapshot.pull_requests[0].checks[0].state = CheckState::Passed;

    judge(&mut wait, 2);
    assert_eq!(ended(&wait), None);
    wait.watch.as_mut().unwrap().snapshot.pull_requests[1].checks[0].state = CheckState::Passed;
    judge(&mut wait, 3);

    assert_eq!(ended(&wait), Some(WaitEnd::ChecksPassed));
}

#[test]
fn merged_dependencies_can_disappear_but_new_or_reordered_ones_end_the_wait() {
    let mut wait = pull_request_wait("a");
    let watch = wait.watch.as_mut().unwrap();
    watch.snapshot.pull_requests[0].dependencies = vec![number(3)];
    watch.snapshot.pull_requests.push(observed_pull_request(3));
    let mut merged = snapshot(&wait);
    merged.pull_requests[0].dependencies.clear();
    merged.pull_requests[1].pull_request.state = PrState::Merged;
    let mut after_merge = wait.clone();

    observe(&mut after_merge, &merged, 2);
    assert_eq!(ended(&after_merge), None);
    assert_eq!(snapshot(&after_merge).pull_requests.len(), 2);
    let mut inserted = snapshot(&wait);
    inserted.pull_requests[0].dependencies.insert(0, number(2));
    inserted.pull_requests.push(observed_pull_request(2));
    observe(&mut wait, &inserted, 2);

    assert_eq!(ended(&wait), Some(WaitEnd::StackChanged));
}

#[test]
fn existing_reviews_are_baselined_but_new_requests_wake() {
    let mut wait = pull_request_wait("a");
    let watch = wait.watch.as_mut().unwrap();
    watch.snapshot.pull_requests[0].pull_request.changes_requested_at_ms = Some(5);
    watch.baseline = baseline(&watch.snapshot);
    judge(&mut wait, 6);
    assert_eq!(ended(&wait), None);
    let mut now = snapshot(&wait);
    now.pull_requests[0].pull_request.changes_requested_at_ms = Some(7);

    observe(&mut wait, &now, 8);

    assert_eq!(ended(&wait), Some(WaitEnd::ChangesRequested));
}

#[test]
fn queue_authority_precedes_transient_conflicts_but_ejection_wakes() {
    let mut wait = Wait { until: WaitUntil::Merged, ..pull_request_wait("a") };
    let row = &mut wait.watch.as_mut().unwrap().snapshot.pull_requests[0];
    row.mergeability = Mergeability::Conflicting;
    row.linear = Some(false);
    row.pull_request.queue = Some(QueueState::AwaitingChecks);
    judge(&mut wait, 2);
    assert_eq!(ended(&wait), None);
    let mut now = snapshot(&wait);
    now.pull_requests[0].pull_request.queue = None;
    now.pull_requests[0].pull_request.left_queue = Some(LeftQueue { at_ms: 3, reason: "failed_checks".into() });

    observe(&mut wait, &now, 4);

    assert_eq!(ended(&wait), Some(WaitEnd::LeftQueue));
}

#[test]
fn drafts_allow_checks_waits_and_non_linear_stacks_need_attention() {
    let mut wait = pull_request_wait("a");
    wait.watch.as_mut().unwrap().snapshot.pull_requests[0].pull_request.is_draft = true;
    let mut merged = Wait { until: WaitUntil::Merged, ..wait.clone() };
    judge(&mut wait, 2);
    judge(&mut merged, 2);
    assert_eq!(ended(&wait), None);
    assert_eq!(ended(&merged), Some(WaitEnd::Draft));
    wait.watch.as_mut().unwrap().snapshot.pull_requests[0].linear = Some(false);
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
    now.pull_requests[0].checks[0].state = CheckState::Passed;

    observe(&mut wait, &now, 5);

    assert_eq!(ended(&wait), Some(WaitEnd::TimedOut));
    assert_eq!(wait, frozen);
}

#[test]
fn a_partial_foreground_read_cannot_turn_a_lower_merge_into_a_stack_change() {
    let mut wait = pull_request_wait("a");
    let watch = wait.watch.as_mut().unwrap();
    watch.snapshot.pull_requests[0].dependencies = vec![number(3)];
    watch.snapshot.pull_requests.push(observed_pull_request(3));
    let before = wait.clone();
    let mut partial = snapshot(&wait);
    partial.pull_requests[0].dependencies.clear();
    partial.pull_requests.pop();

    observe(&mut wait, &partial, 2);
    assert_eq!(wait, before);
    let mut merged = observed_pull_request(3);
    merged.pull_request.state = PrState::Merged;
    partial.pull_requests.push(merged);
    observe(&mut wait, &partial, 3);

    assert_eq!(ended(&wait), None);
    assert_eq!(snapshot(&wait).pull_requests[1].pull_request.state, PrState::Merged);
}
