//! When the tail says the agent is trying a failed request again, and when it stops saying so.

use super::tests::turn_ended;
use super::*;
use crate::event::{ExitReason, FailureKind, RequestFailure};
use crate::tail::Retrying;

fn retry(attempt: u32) -> Retrying {
    Retrying { run: None, attempt, of: Some(10), status: Some(503), delay_ms: Some(1_200) }
}

/// A tail whose agent, of run 1, is trying a request a second time.
fn retrying_in_run_1() -> Tail {
    let mut tail = Tail::default();
    tail.retrying(RunId::new(1), retry(2));
    tail
}

/// What the agent itself, or `call`'s subagent, writes.
fn writes(call: Option<&str>) -> EventBody {
    EventBody::Assistant {
        message_id: Some("m1".into()),
        subagent: call.map(Into::into),
        model: None,
        blocks: vec![],
        first_index: None,
    }
}

/// The first of what the agent itself, or `call`'s subagent, streams.
fn streams(call: Option<&str>) -> StreamDelta {
    StreamDelta {
        run: Some(RunId::new(1)),
        message_id: MessageId::from("m1"),
        subagent: call.map(Into::into),
        change: DeltaChange::Text { index: 0, text: "Here".into() },
    }
}

/// A request of the agent's, or `call`'s subagent's, that failed for good.
fn failed(call: Option<&str>) -> EventBody {
    let failure = RequestFailure { message: "Overloaded".into(), kind: FailureKind::Unknown, resets_at_ms: None };
    EventBody::RequestFailed { subagent: call.map(Into::into), failure, again: None }
}

fn exited() -> EventBody {
    EventBody::AgentExited { code: None, signal: None, reason: ExitReason::Requested, stderr_tail: vec![] }
}

#[test]
fn the_tail_says_which_try_is_next_with_its_run() {
    let mut tail = retrying_in_run_1();
    tail.retrying(RunId::new(1), retry(3));

    assert_eq!(tail.retrying, Some(Retrying { run: Some(RunId::new(1)), ..retry(3) }));
    assert!(!tail.is_empty(), "retrying is something to show");
    assert!(!tail.holds_nothing());
    assert_eq!(tail.runs(), [RunId::new(1)]);
}

#[test]
fn the_agent_writing_failing_for_good_or_its_turn_or_run_ending_ends_its_retrying() {
    for over in [writes(None), failed(None), turn_ended(), exited()] {
        let mut tail = retrying_in_run_1();

        tail.on_event(Seq::new(2), Some(RunId::new(1)), &over);

        assert_eq!(tail.retrying, None, "{over:?}");
    }
    let mut tail = retrying_in_run_1();
    tail.apply(&streams(None));
    assert_eq!(tail.retrying, None, "the agent streams its answer");
}

#[test]
fn a_subagent_writing_or_failing_doesnt_end_the_agents_retrying() {
    let mut tail = retrying_in_run_1();

    tail.on_event(Seq::new(2), Some(RunId::new(1)), &writes(Some("toolu-1")));
    tail.on_event(Seq::new(3), Some(RunId::new(1)), &failed(Some("toolu-1")));
    tail.apply(&streams(Some("toolu-1")));

    assert!(tail.retrying.is_some());
}

#[test]
fn an_earlier_runs_end_doesnt_end_a_later_runs_retrying() {
    let mut tail = Tail::default();
    tail.retrying(RunId::new(2), retry(2));

    for over in [writes(None), failed(None), turn_ended(), exited()] {
        tail.on_event(Seq::new(2), Some(RunId::new(1)), &over);
    }

    assert!(tail.retrying.is_some());
}

#[test]
fn only_the_agents_own_tail_and_its_runs_say_it_retries() {
    let tail = retrying_in_run_1();

    assert!(tail.of(None).retrying.is_some());
    assert!(tail.of(Some(&"toolu-1".into())).retrying.is_none(), "a subagent's tab");
    assert!(tail.of_run(RunId::new(1)).retrying.is_some());
    assert!(tail.of_run(RunId::new(2)).retrying.is_none());

    let mut hub = Tail::default();
    hub.replace_run(RunId::new(1), tail.of_run(RunId::new(1)));
    assert_eq!(hub.retrying, tail.retrying, "a hub takes it from the worker it connects to");
    hub.replace_run(RunId::new(1), Tail::default());
    assert_eq!(hub.retrying, None, "and the worker's saying none ends it");
}
