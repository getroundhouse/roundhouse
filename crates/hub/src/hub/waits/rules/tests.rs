use protocol::model::{Checks, LeftQueue, Local, Review};

use super::super::tests::{open, wait};
use super::*;

/// When GitHub last recorded the pull request leaving the queue, and a review asking for changes, as the wait started,
/// by GitHub's clock, which records to the second.
const SEEN_AT: i64 = 1_000;
const BEFORE: i64 = 0;
const AFTER: i64 = 2_000;

const SEEN: Seen = Seen { left_queue_at_ms: Some(SEEN_AT), changes_requested_at_ms: Some(SEEN_AT) };

fn queued() -> PullRequest {
    PullRequest { queue: Some(QueueState::AwaitingChecks), ..open() }
}

fn left(at_ms: i64) -> LeftQueue {
    LeftQueue { at_ms, reason: "failed_checks".into() }
}

/// How a wait until it's done, started from [`SEEN`], ends when a read finds `now`.
fn done(now: &PullRequest) -> Option<WaitEnd> {
    ended(&wait(WaitUntil::Done), SEEN, Some(now))
}

fn checks(now: &PullRequest) -> Option<WaitEnd> {
    ended(&wait(WaitUntil::Checks), SEEN, Some(now))
}

#[test]
fn a_pull_request_thats_done_ends_it() {
    let merged = PullRequest { state: PrState::Merged, ..queued() };
    let closed = PullRequest { state: PrState::Closed, ..open() };
    let another = PullRequest { number: 8, ..open() };

    assert_eq!(done(&merged), Some(WaitEnd::Merged));
    assert_eq!(done(&closed), Some(WaitEnd::Closed));
    assert_eq!(ended(&wait(WaitUntil::Done), SEEN, None), Some(WaitEnd::NoPullRequest));
    assert_eq!(done(&another), Some(WaitEnd::NoPullRequest), "the branch's is another one now");
}

#[test]
fn the_queue_merges_it_whatever_else_is_true_so_it_waits_until_it_leaves() {
    let conflicting = PullRequest { merge_state: MergeState::Dirty, ..queued() };
    let failing = PullRequest { checks: Checks { passed: 1, failed: 1, pending: 0 }, ..queued() };
    let unmergeable = PullRequest { queue: Some(QueueState::Unmergeable), ..queued() };
    let back_in = PullRequest { left_queue: Some(left(AFTER)), ..queued() };

    assert_eq!(done(&conflicting), None, "as a stack's upper pull request reads while both merge");
    assert_eq!(done(&failing), None);
    assert_eq!(done(&back_in), None, "out and back in again");
    assert_eq!(done(&unmergeable), Some(WaitEnd::LeftQueue));
}

#[test]
fn leaving_the_queue_since_the_wait_started_ends_it_as_github_records_it_even_when_no_read_saw_it_there() {
    let taken_out = PullRequest { left_queue: Some(left(AFTER)), ..open() };
    let seen = PullRequest { left_queue: Some(left(SEEN_AT)), ..open() };

    assert_eq!(done(&taken_out), Some(WaitEnd::LeftQueue), "in and out between two reads, or while one was read");
    assert_eq!(done(&seen), None, "it left before the wait started");
}

/// So neither the hub's clock, nor GitHub's recording only to the second, makes what came before the wait end it.
#[test]
fn what_github_records_is_judged_against_what_it_had_recorded_as_the_wait_started() {
    let left_at = |at_ms| PullRequest { left_queue: Some(left(at_ms)), ..open() };
    let asked_at = |at_ms| PullRequest { changes_requested_at_ms: Some(at_ms), ..open() };
    let from_nothing = |now: &PullRequest| ended(&wait(WaitUntil::Done), Seen::default(), Some(now));

    assert_eq!(done(&left_at(SEEN_AT)), None, "the one it had seen");
    assert_eq!(done(&asked_at(SEEN_AT)), None);
    assert_eq!(done(&left_at(BEFORE)), None, "older still, as an older read can say");
    assert_eq!(from_nothing(&left_at(BEFORE)), Some(WaitEnd::LeftQueue), "however early it's dated");
    assert_eq!(from_nothing(&asked_at(BEFORE)), Some(WaitEnd::ChangesRequested));
}

#[test]
fn a_draft_ends_a_wait_until_its_done_since_it_cant_merge_but_not_one_until_its_checks() {
    let draft = PullRequest { is_draft: true, merge_state: MergeState::Draft, ..open() };
    let passed = PullRequest { checks: Checks { passed: 3, failed: 0, pending: 0 }, ..draft.clone() };
    let unpushed = PullRequest { local: Local { ahead: 1, ..Local::default() }, ..draft.clone() };

    assert_eq!(done(&draft), Some(WaitEnd::Draft));
    assert_eq!(done(&unpushed), Some(WaitEnd::Draft), "whatever the worktree has");
    assert_eq!(checks(&draft), None, "still running");
    assert_eq!(checks(&passed), Some(WaitEnd::ChecksPassed));
}

#[test]
fn what_only_the_agent_can_deal_with_ends_it() {
    let conflicts = PullRequest { merge_state: MergeState::Dirty, ..open() };
    let changes = PullRequest { changes_requested_at_ms: Some(AFTER), ..open() };
    let failed = PullRequest { checks: Checks { passed: 2, failed: 1, pending: 1 }, ..open() };

    assert_eq!(done(&conflicts), Some(WaitEnd::Conflicts));
    assert_eq!(done(&changes), Some(WaitEnd::ChangesRequested));
    assert_eq!(done(&failed), Some(WaitEnd::ChecksFailed), "even with one still running, while GitHub holds it back");
}

/// GitHub's decision stays at changes requested through every later review that asks again.
#[test]
fn a_review_asking_for_changes_counts_once_its_asked_since_the_wait_started() {
    let decided = |at_ms| PullRequest { review: Review::ChangesRequested, changes_requested_at_ms: at_ms, ..open() };

    assert_eq!(done(&decided(Some(SEEN_AT))), None, "as when the agent has pushed what was asked for");
    assert_eq!(done(&decided(Some(AFTER))), Some(WaitEnd::ChangesRequested), "asked again");
}

#[test]
fn what_github_says_of_an_older_commit_ends_nothing_until_the_agents_are_pushed() {
    let unpushed = |pr: PullRequest| PullRequest { local: Local { ahead: 1, ..Local::default() }, ..pr };
    let conflicts = unpushed(PullRequest { merge_state: MergeState::Dirty, ..open() });
    let failed = unpushed(PullRequest { checks: Checks { passed: 2, failed: 1, pending: 0 }, ..open() });
    let merged = unpushed(PullRequest { state: PrState::Merged, ..open() });
    let changes = unpushed(PullRequest { changes_requested_at_ms: Some(AFTER), ..open() });

    assert_eq!(done(&conflicts), None);
    assert_eq!(done(&failed), None);
    assert_eq!(checks(&failed), None);
    assert_eq!(done(&merged), Some(WaitEnd::Merged));
    assert_eq!(done(&changes), Some(WaitEnd::ChangesRequested), "a review is of the pull request");
}

#[test]
fn a_failed_check_ends_a_wait_until_done_only_once_github_says_it_holds_the_merge_back() {
    let failed =
        |merge_state| PullRequest { checks: Checks { passed: 2, failed: 1, pending: 0 }, merge_state, ..open() };

    assert_eq!(done(&failed(MergeState::Unstable)), None, "it can still merge");
    assert_eq!(done(&failed(MergeState::Computing)), None, "GitHub is still working it out");
    assert_eq!(done(&failed(MergeState::Blocked)), Some(WaitEnd::ChecksFailed));
    assert_eq!(checks(&failed(MergeState::Unstable)), Some(WaitEnd::ChecksFailed), "a wait until the checks");
}

#[test]
fn what_the_agent_is_doing_in_the_worktree_ends_nothing() {
    let working = PullRequest { local: Local { uncommitted: 3, ahead: 0, behind: 1 }, ..open() };
    let approved = PullRequest { review: Review::Approved, merge_state: MergeState::Clean, ..open() };
    let unknown = PullRequest { state: PrState::Unknown, ..open() };

    assert_eq!(done(&working), None);
    assert_eq!(done(&approved), None, "waiting for it to merge");
    assert_eq!(done(&unknown), None);
}

#[test]
fn a_wait_until_its_checks_ends_once_they_have_run_and_none_failed() {
    let passed = PullRequest { checks: Checks { passed: 3, failed: 0, pending: 0 }, ..open() };
    let not_started = PullRequest { checks: Checks::default(), ..open() };

    assert_eq!(checks(&passed), Some(WaitEnd::ChecksPassed));
    assert_eq!(checks(&open()), None, "still running");
    assert_eq!(checks(&not_started), None, "not started yet, as just after a push");
    assert_eq!(done(&passed), None, "a wait until it's done waits on");
}

#[test]
fn commits_not_pushed_hold_a_wait_back_unless_the_queue_has_it() {
    let unpushed = |ahead, behind| PullRequest { local: Local { uncommitted: 0, ahead, behind }, ..open() };

    assert_eq!(not_yet(&unpushed(0, 3)), None, "commits to pull change nothing GitHub says");
    assert_eq!(
        not_yet(&unpushed(1, 0)).as_deref(),
        Some("1 commit here isn't on the pull request yet, so it would wait on an older commit. Push, then wait.")
    );
    assert!(not_yet(&unpushed(2, 1)).unwrap().contains("It has 1 commit that isn't here, so bring them together"));
    let queued = PullRequest { queue: Some(QueueState::Queued), ..unpushed(2, 0) };
    assert_eq!(not_yet(&queued), None, "the queue merges what's on GitHub");
}
