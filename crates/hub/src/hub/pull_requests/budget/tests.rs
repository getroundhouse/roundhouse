use std::time::{Duration, Instant};

use protocol::model::{Checks, Local, MergeState, PrState, PullRequest, PullRequestFound, Review};
use protocol::worker::RateLimit;

use super::{Asked, BURST, Budget, GITHUB_SAID_FOR, PER_MINUTE, calls_to_read};

const HOUR: Duration = Duration::from_hours(1);

/// The calls that go one after another, as reads nobody waits on, before one has to wait.
fn calls_in_a_row(budget: &mut Budget, now: Instant) -> u32 {
    let mut calls = 0;
    while budget.take(1, Asked::InTurn, now).is_ok() {
        calls += 1;
    }
    calls
}

/// GitHub saying this much is left of its 5,000 an hour, resetting `resets_in` from now.
fn github_says(budget: &mut Budget, remaining: u32, resets_in: Duration, now: Instant) {
    let resets_at_ms = 1_000 + i64::try_from(resets_in.as_millis()).unwrap();
    budget.github_said(RateLimit { limit: 5000, remaining, resets_at_ms }, now, 1_000);
}

#[test]
fn a_burst_goes_at_once_and_then_the_rest_go_at_the_steady_rate() {
    let now = Instant::now();
    let mut budget = Budget::new(now);
    let a_call = Duration::from_secs_f64(60.0 / f64::from(PER_MINUTE));

    let at_once = calls_in_a_row(&mut budget, now);
    let waits_until = budget.allows(1, Asked::InTurn, now).unwrap_err();
    let a_call_later = calls_in_a_row(&mut budget, now + a_call);

    assert_eq!(at_once, BURST);
    assert_eq!(waits_until, now + a_call);
    assert_eq!(a_call_later, 1);
}

/// What someone is waiting on doesn't wait for the steady rate, but it's counted, so the rest wait longer.
#[test]
fn a_read_someone_waits_on_goes_ahead_and_holds_back_the_rest() {
    let now = Instant::now();
    let mut budget = Budget::new(now);
    calls_in_a_row(&mut budget, now);

    let went = budget.take(2, Asked::Waiting, now);
    let after = budget.allows(1, Asked::InTurn, now).unwrap_err();

    assert_eq!(went, Ok(()));
    assert_eq!(after, now + Duration::from_secs_f64(3.0 * 60.0 / f64::from(PER_MINUTE)));
}

/// Asking only says, so a read that asks and then doesn't go spends nothing.
#[test]
fn asking_whether_a_read_can_go_spends_nothing() {
    let now = Instant::now();
    let mut budget = Budget::new(now);

    for _ in 0..BURST * 2 {
        assert_eq!(budget.allows(1, Asked::InTurn, now), Ok(()));
    }

    assert_eq!(calls_in_a_row(&mut budget, now), BURST);
}

#[test]
fn reads_nobody_waits_on_leave_a_fifth_of_githubs_limit_alone_until_it_resets() {
    let now = Instant::now();
    let mut budget = Budget::new(now);

    github_says(&mut budget, 1001, HOUR, now);
    let while_above = budget.take(1, Asked::InTurn, now);
    let at_the_fifth = budget.allows(1, Asked::InTurn, now);
    let waiting = budget.allows(1, Asked::Waiting, now);
    let once_reset = budget.allows(1, Asked::InTurn, now + HOUR);

    assert_eq!(while_above, Ok(()));
    assert_eq!(at_the_fifth, Err(now + HOUR), "the hub's own call since counts against what's left");
    assert_eq!(waiting, Ok(()), "what's kept for others is still there for a read someone waits on");
    assert_eq!(once_reset, Ok(()));
}

/// An agent's read goes at once, as one someone waits on does, but it can't spend what's kept for everything else.
#[test]
fn an_agents_read_goes_ahead_but_leaves_a_fifth_of_githubs_limit_alone() {
    let now = Instant::now();
    let mut budget = Budget::new(now);
    calls_in_a_row(&mut budget, now);

    let ahead = budget.take(2, Asked::Agent, now);
    github_says(&mut budget, 1000, HOUR, now);
    let at_the_fifth = budget.allows(1, Asked::Agent, now);

    assert_eq!(ahead, Ok(()), "past the steady rate");
    assert_eq!(at_the_fifth, Err(now + HOUR));
}

#[test]
fn nothing_goes_while_github_has_nothing_left() {
    let now = Instant::now();
    let mut budget = Budget::new(now);

    github_says(&mut budget, 0, HOUR, now);

    assert_eq!(budget.take(1, Asked::Waiting, now), Err(now + HOUR));
    assert_eq!(budget.allows(1, Asked::InTurn, now), Err(now + HOUR));
    assert_eq!(budget.take(1, Asked::Waiting, now + HOUR), Ok(()), "it resets");
}

#[test]
fn github_is_asked_again_once_what_it_said_is_a_minute_old_or_has_reset() {
    let now = Instant::now();
    let mut budget = Budget::new(now);
    assert!(budget.needs_github(now), "nothing is known yet");

    github_says(&mut budget, 4000, HOUR, now);
    let just_under = now + GITHUB_SAID_FOR.checked_sub(Duration::from_millis(1)).unwrap();
    let (fresh, old) = (budget.needs_github(just_under), budget.needs_github(now + GITHUB_SAID_FOR));
    github_says(&mut budget, 0, Duration::from_secs(10), now);

    assert!(!fresh);
    assert!(old);
    assert!(budget.needs_github(now + Duration::from_secs(10)), "it has reset");
}

/// Where the clock can't count that far, as a Mac's can't, it says nothing, rather than stopping the hub.
#[test]
fn a_reset_too_far_off_to_count_to_is_taken_in_stride() {
    let now = Instant::now();
    let mut budget = Budget::new(now);

    budget.github_said(RateLimit { limit: 5000, remaining: 0, resets_at_ms: i64::MAX }, now, i64::MIN);

    assert!(!budget.needs_github(now), "it was asked");
}

#[test]
fn an_open_pull_request_or_one_never_read_takes_two_calls_and_anything_else_one() {
    let pr = |state, is_draft| PullRequest {
        number: 7,
        url: "https://example.com/pull/7".to_string(),
        title: "T".to_string(),
        state,
        is_draft,
        base: None,
        merge_state: MergeState::Clean,
        review: Review::default(),
        checks: Checks::default(),
        queue: None,
        left_queue: None,
        changes_requested_at_ms: None,
        local: Local::default(),
    };
    let found = |pr: Option<PullRequest>| Some(PullRequestFound { pull_request: pr });

    assert_eq!(calls_to_read(found(Some(pr(PrState::Open, false))).as_ref()), 2);
    assert_eq!(calls_to_read(None), 2);
    assert_eq!(calls_to_read(found(Some(pr(PrState::Open, true))).as_ref()), 2, "a draft too");
    assert_eq!(calls_to_read(found(Some(pr(PrState::Merged, false))).as_ref()), 1);
    assert_eq!(calls_to_read(found(None).as_ref()), 1);
}
