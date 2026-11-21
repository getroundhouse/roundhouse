//! When the tail says what the agent is doing, said for it while it can't answer, and when it stops: only when its
//! adapter closes the wait, or the agent exits.

use super::tests::turn_ended;
use super::*;
use crate::capability::output::{CoverLine, CoverWait};
use crate::event::{ContentBlock, ExitReason};
use crate::tail::Covering;

fn line(wait: u32, number: u32) -> CoverLine {
    CoverLine { wait: CoverWait(wait), number, text: format!("Hold on, line {number} of wait {wait}.") }
}

/// A tail whose agent, of run 1, has the first line of its first wait said for it.
fn covered_in_run_1() -> Tail {
    let mut tail = Tail::default();
    tail.covering(RunId::new(1), line(1, 1));
    tail
}

fn said(tail: &Tail) -> Option<(Option<RunId>, CoverLine)> {
    tail.covering.as_deref().map(|Covering { run, line }| (*run, line.clone()))
}

fn exited() -> EventBody {
    EventBody::AgentExited { code: None, signal: None, reason: ExitReason::Requested, stderr_tail: vec![] }
}

/// The agent answering, in words of its own.
fn answers() -> EventBody {
    let blocks = vec![ContentBlock::Text { text: "Found it.".into(), images: Vec::new() }];
    EventBody::Assistant { message_id: Some("m1".into()), subagent: None, model: None, blocks, first_index: None }
}

#[test]
fn the_tail_says_the_newest_line_with_its_run_and_holds_something_while_it_does() {
    let mut tail = covered_in_run_1();
    tail.covering(RunId::new(1), line(1, 2));

    assert_eq!(said(&tail), Some((Some(RunId::new(1)), line(1, 2))));
    assert!(tail.is_empty(), "a line said for the agent isn't the agent writing");
    assert!(!tail.holds_nothing());
    assert_eq!(tail.runs(), [RunId::new(1)]);

    tail.covering(RunId::new(1), line(2, 1));
    assert_eq!(said(&tail), Some((Some(RunId::new(1)), line(2, 1))), "the next wait's takes its place");
}

/// Only the adapter knows whether the agent has answered what the line covers, so neither its words nor its turn's
/// end does: it closes the line's wait, and an older wait closing leaves the newer one's line.
#[test]
fn only_its_wait_closing_or_its_agent_exiting_ends_the_line() {
    let mut tail = covered_in_run_1();
    let streams = StreamDelta {
        run: Some(RunId::new(1)),
        message_id: MessageId::from("m1"),
        subagent: None,
        change: DeltaChange::Text { index: 0, text: "Found".into() },
    };

    tail.apply(&streams);
    tail.on_event(Seq::new(2), Some(RunId::new(1)), &answers());
    tail.on_event(Seq::new(3), Some(RunId::new(1)), &turn_ended());
    tail.covered(RunId::new(1), CoverWait(0));
    tail.covered(RunId::new(2), CoverWait(1));
    assert!(tail.covering.is_some());

    tail.covered(RunId::new(1), CoverWait(1));
    assert_eq!(tail.covering, None);

    let mut tail = covered_in_run_1();
    tail.on_event(Seq::new(2), Some(RunId::new(1)), &exited());
    assert_eq!(tail.covering, None, "nothing will close its wait once the agent has gone");
    let mut tail = Tail::default();
    tail.covering(RunId::new(2), line(1, 1));
    tail.on_event(Seq::new(2), Some(RunId::new(1)), &exited());
    assert!(tail.covering.is_some(), "an earlier run's exit leaves a later run's line");
}

#[test]
fn only_the_agents_own_tail_and_its_runs_have_the_line_and_a_worker_hands_it_over() {
    let tail = covered_in_run_1();

    assert!(tail.of(None).covering.is_some());
    assert!(tail.of(Some(&"toolu-1".into())).covering.is_none(), "a subagent's tab");
    assert!(tail.of_run(RunId::new(1)).covering.is_some());
    assert!(tail.of_run(RunId::new(2)).covering.is_none());

    let mut hub = Tail::default();
    hub.replace_run(RunId::new(1), tail.of_run(RunId::new(1)));
    assert_eq!(hub.covering, tail.covering, "a hub takes it from the worker it connects to");
    hub.replace_run(RunId::new(1), Tail::default());
    assert_eq!(hub.covering, None, "and the worker's saying none ends it");
}
