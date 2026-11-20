//! When the tail says what the agent is doing, said for it while it can't answer a prompt said out loud, and when it
//! stops saying so.

use super::tests::turn_ended;
use super::*;
use crate::event::{ContentBlock, ExitReason};
use crate::tail::Covering;

fn line(prompt: u64) -> Covering {
    Covering { run: None, prompt: Seq::new(prompt), line: "Hold on, I'm reading the failing test.".into() }
}

/// A tail whose agent, of run 1, is covered for while it can't answer the prompt at 4.
fn covered_in_run_1() -> Tail {
    let mut tail = Tail::default();
    tail.covering(RunId::new(1), line(4));
    tail
}

/// What the agent itself, or `call`'s subagent, records: words, or a call.
fn says(call: Option<&str>, words: bool) -> EventBody {
    let block = if words {
        ContentBlock::Text { text: "Found it.".into(), images: Vec::new() }
    } else {
        ContentBlock::Thinking { text: "Which test?".into() }
    };
    EventBody::Assistant {
        message_id: Some("m1".into()),
        subagent: call.map(Into::into),
        model: None,
        blocks: vec![block],
        first_index: None,
    }
}

/// The first of what the agent itself, or `call`'s subagent, streams.
fn streams(call: Option<&str>, change: DeltaChange) -> StreamDelta {
    StreamDelta { run: Some(RunId::new(1)), message_id: MessageId::from("m1"), subagent: call.map(Into::into), change }
}

fn exited() -> EventBody {
    EventBody::AgentExited { code: None, signal: None, reason: ExitReason::Requested, stderr_tail: vec![] }
}

#[test]
fn the_tail_says_the_line_with_its_run_and_its_prompt() {
    let tail = covered_in_run_1();

    assert_eq!(tail.covering.as_deref(), Some(&Covering { run: Some(RunId::new(1)), ..line(4) }));
    assert!(!tail.is_empty(), "the line is something to show");
    assert!(!tail.holds_nothing());
    assert_eq!(tail.runs(), [RunId::new(1)]);
}

#[test]
fn the_agent_starting_to_answer_or_its_turn_or_run_ending_ends_the_line() {
    for over in [says(None, true), turn_ended(), exited()] {
        let mut tail = covered_in_run_1();

        tail.on_event(Seq::new(5), Some(RunId::new(1)), &over);

        assert_eq!(tail.covering, None, "{over:?}");
    }
    let text = DeltaChange::Text { index: 0, text: "Found".into() };
    let starts = DeltaChange::BlockStart { index: 0, kind: BlockKind::Text, tool_name: None };
    for change in [text, starts] {
        let mut tail = covered_in_run_1();
        tail.apply(&streams(None, change.clone()));
        assert_eq!(tail.covering, None, "the agent streams its answer: {change:?}");
    }
}

/// The agent thinking, calling a tool or having a subagent write is the agent still busy, which the line covers.
#[test]
fn thinking_a_call_or_a_subagent_writing_leaves_the_line() {
    let mut tail = covered_in_run_1();

    tail.on_event(Seq::new(5), Some(RunId::new(1)), &says(None, false));
    tail.on_event(Seq::new(6), Some(RunId::new(1)), &says(Some("toolu-1"), true));
    tail.apply(&streams(None, DeltaChange::Thinking { index: 0, text: "Hmm".into() }));
    tail.apply(&streams(None, DeltaChange::ToolInput { index: 1, json: "{".into() }));
    tail.apply(&streams(Some("toolu-1"), DeltaChange::Text { index: 0, text: "Found".into() }));

    assert_eq!(tail.covering.as_deref(), Some(&Covering { run: Some(RunId::new(1)), ..line(4) }));
}

#[test]
fn an_earlier_runs_end_leaves_a_later_runs_line() {
    let mut tail = Tail::default();
    tail.covering(RunId::new(2), line(4));

    tail.on_event(Seq::new(5), Some(RunId::new(1)), &turn_ended());

    assert!(tail.covering.is_some());
}

/// Only the agent's own replies are answered out loud, so a subagent's tail never has a line. A worker hands its run's
/// line to a hub that connects again, which takes it as that run's.
#[test]
fn a_subagents_tail_has_no_line_and_a_worker_hands_on_its_runs() {
    let tail = covered_in_run_1();

    assert_eq!(tail.of(Some(&"toolu-1".into())).covering, None);
    assert!(tail.of(None).covering.is_some());
    assert_eq!(tail.of_run(RunId::new(2)).covering, None);
    let mut hub = Tail::default();
    hub.replace_run(RunId::new(1), tail.of_run(RunId::new(1)));
    assert_eq!(hub.covering, tail.covering);
    hub.replace_run(RunId::new(1), Tail::default());
    assert_eq!(hub.covering, None, "the worker's says it's over");
}
