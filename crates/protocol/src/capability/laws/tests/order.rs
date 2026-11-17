use super::super::*;
use crate::event::{EventBody, TurnOutcome};
use crate::fixtures::{subagent_started, turn_ended};

use super::log::*;

#[test]
fn a_session_that_keeps_every_law_breaks_none() {
    let log = [
        from_run(1, 1, started()),
        from_hub(2, prompt()),
        from_run(1, 3, taken(2)),
        from_run(1, 4, said(vec![call("t")])),
        from_run(1, 5, asked("a")),
        from_hub(6, resolved("a")),
        from_run(1, 7, resolved("a")),
        from_run(1, 8, sent_back(vec![result("t")])),
        from_run(1, 9, subagent_started("t", false)),
        from_run(1, 10, turn_ended(TurnOutcome::Success)),
        from_hub(11, prompt()),
        from_run(1, 12, exited()),
        from_hub(13, dropped(11)),
        from_hub(14, subagent_ended("t")),
        from_run(2, 15, started()),
        from_run(2, 16, taken(11)),
        from_run(2, 17, subagent_ended("t")),
    ];

    assert_eq!(breaches(&log, &claims()), []);
}

#[test]
fn a_prompt_is_settled_after_it_was_recorded_by_the_hub_before_the_agent_and_by_each_once() {
    let asked_to_compact = EventBody::CompactionAsked { keeping: None };
    let log = [
        from_hub(1, prompt()),
        from_hub(2, prompt()),
        from_hub(3, dropped(1)),
        from_run(1, 4, taken(1)),
        from_run(1, 5, taken(1)),
        from_run(1, 6, taken(2)),
        from_hub(7, dropped(2)),
        from_run(1, 8, taken(9)),
        from_hub(9, asked_to_compact),
        from_run(1, 10, taken(9)),
    ];

    let expected = [5, 7, 8].map(|seq| (Law::PromptSettlesOnce, seq));
    assert_eq!(breaches(&log, &claims()), expected);
}

#[test]
fn a_request_is_resolved_after_it_was_made_by_the_hub_before_the_agent_and_by_each_once() {
    let log = [
        from_hub(1, resolved("a")),
        from_run(1, 2, asked("a")),
        from_hub(3, resolved("a")),
        from_run(1, 4, resolved("a")),
        from_run(1, 5, resolved("a")),
        from_run(1, 6, asked("b")),
        from_run(1, 7, resolved("b")),
        from_hub(8, resolved("b")),
    ];

    let expected = [1, 5, 8].map(|seq| (Law::ApprovalResolvesOnce, seq));
    assert_eq!(breaches(&log, &claims()), expected);
}

/// Each time it starts, as one given more work starts again once it's ended.
#[test]
fn a_subagent_ends_after_it_started_by_the_hub_before_the_agent_and_by_each_once() {
    let log = [
        from_run(1, 1, subagent_ended("t")),
        from_run(1, 2, subagent_started("t", false)),
        from_hub(3, subagent_ended("t")),
        from_run(1, 4, subagent_ended("t")),
        from_run(1, 5, subagent_ended("t")),
        from_run(1, 6, subagent_started("u", false)),
        from_hub(7, subagent_ended("u")),
        from_hub(8, subagent_ended("u")),
        from_run(1, 9, subagent_started("v", false)),
        from_run(1, 10, subagent_ended("v")),
        from_hub(11, subagent_ended("v")),
        from_run(1, 12, subagent_started("v", false)),
        from_hub(13, subagent_ended("v")),
        from_run(1, 14, subagent_ended("v")),
        from_run(1, 15, subagent_started("v", false)),
        from_run(1, 16, subagent_ended("v")),
        from_run(1, 17, subagent_ended("v")),
    ];

    let expected = [1, 5, 8, 11, 17].map(|seq| (Law::SubagentEndsOnce, seq));
    assert_eq!(breaches(&log, &claims()), expected);
}

#[test]
fn a_call_has_one_result_and_only_after_it() {
    let log = [
        from_run(1, 1, sent_back(vec![result("early")])),
        from_run(1, 2, said(vec![call("t")])),
        from_run(1, 3, sent_back(vec![result("t")])),
        from_run(1, 4, sent_back(vec![result("t")])),
    ];

    assert_eq!(breaches(&log, &claims()), [(Law::ResultFollowsCall, 1), (Law::ResultFollowsCall, 4)]);
}

#[test]
fn nothing_of_a_run_comes_after_it_exits_but_its_start_and_the_hub_still_records() {
    let log = [
        from_run(1, 1, asked("a")),
        from_run(1, 2, exited()),
        from_hub(3, resolved("a")),
        from_run(1, 4, turn_ended(TurnOutcome::Success)),
        from_run(1, 5, said(vec![])),
        from_run(1, 6, asked("b")),
        from_run(1, 7, started()),
        from_run(1, 8, exited()),
        from_run(2, 9, turn_ended(TurnOutcome::Success)),
    ];

    let expected = [4, 5, 6, 8].map(|seq| (Law::NothingAfterExit, seq));
    assert_eq!(breaches(&log, &claims()), expected);
}
