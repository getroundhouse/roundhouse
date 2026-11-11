//! When the tail says the agent is compacting, and when it stops saying so.

use super::*;
use crate::event::{CompactionOutcome, ExitReason, TurnOutcome};

fn compacting(written_chars: Option<u64>) -> EventBody {
    EventBody::AgentCompacting { written_chars }
}

fn compacted() -> EventBody {
    EventBody::AgentCompacted { asked: None, keeping: None, outcome: CompactionOutcome::Unknown }
}

/// What the agent itself, or `call`'s subagent, writes.
fn writes(call: Option<&str>) -> EventBody {
    EventBody::Assistant {
        message_id: Some("m1".into()),
        parent_tool_use_id: call.map(Into::into),
        model: None,
        blocks: vec![],
        first_index: None,
    }
}

fn turn_ended() -> EventBody {
    EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: vec![],
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
        started_at_ms: None,
        models: vec![],
        tokens: None,
        only_compacted: false,
    }
}

fn exited() -> EventBody {
    EventBody::AgentExited { code: None, signal: None, reason: ExitReason::Requested, stderr_tail: vec![] }
}

/// A tail that the agent of run 1 says is compacting.
fn compacting_in_run_1() -> Tail {
    let mut tail = Tail::default();
    tail.on_event(1, Some(RunId::new(1)), &compacting(None));
    tail
}

/// A tail whose agent the hub has asked to compact, at 5, which it hasn't said it's doing yet.
fn asked_at_5() -> Tail {
    let mut tail = Tail::default();
    tail.on_event(5, None, &EventBody::CompactionAsked { keeping: None });
    tail
}

#[test]
fn the_tail_says_how_much_is_written_until_the_compaction_says_its_done() {
    let mut tail = compacting_in_run_1();
    tail.on_event(2, Some(RunId::new(1)), &compacting(Some(8_463)));

    assert_eq!(tail.compacting, Some(Compacting { run: Some(RunId::new(1)), asked: None, written_chars: Some(8_463) }));
    assert!(!tail.is_empty(), "compacting is something to show");
    tail.on_event(3, Some(RunId::new(1)), &compacted());
    assert_eq!(tail.compacting, None);
}

#[test]
fn the_agent_writing_or_its_turn_or_run_ending_ends_a_compaction_whose_end_never_came() {
    for over in [writes(None), turn_ended(), exited()] {
        let mut tail = compacting_in_run_1();

        tail.on_event(2, Some(RunId::new(1)), &over);

        assert_eq!(tail.compacting, None, "{over:?}");
    }
}

#[test]
fn a_subagent_writing_doesnt_end_the_agents_compaction() {
    let mut tail = compacting_in_run_1();

    tail.on_event(2, Some(RunId::new(1)), &writes(Some("toolu-1")));

    assert!(tail.compacting.is_some());
}

#[test]
fn an_earlier_runs_end_doesnt_end_a_later_runs_compaction() {
    let mut tail = Tail::default();
    tail.on_event(1, Some(RunId::new(2)), &compacting(None));

    for over in [writes(None), turn_ended(), exited(), compacted()] {
        tail.on_event(2, Some(RunId::new(1)), &over);
    }

    assert!(tail.compacting.is_some());
}

#[test]
fn only_the_agents_own_tail_says_its_compacting() {
    let tail = compacting_in_run_1();

    assert!(tail.of(None).compacting.is_some());
    assert!(tail.of(Some(&"toolu-1".into())).compacting.is_none());
}

#[test]
fn a_compaction_asked_for_is_under_way_until_the_agent_says_it_is_and_then_as_it_says() {
    let mut tail = asked_at_5();

    assert_eq!(tail.compacting, Some(Compacting { run: None, asked: Some(5), written_chars: None }));
    assert!(!tail.is_empty(), "so the wait for the agent says what it's waiting for");
    tail.on_event(6, Some(RunId::new(2)), &compacting(Some(120)));
    assert_eq!(tail.compacting, Some(Compacting { run: Some(RunId::new(2)), asked: None, written_chars: Some(120) }));
}

#[test]
fn a_compaction_asked_for_ends_when_its_request_is_dropped_and_not_when_another_is() {
    let mut tail = asked_at_5();

    tail.on_event(7, None, &EventBody::PromptDropped { prompt: 6 });
    assert!(tail.compacting.is_some(), "another prompt's");
    tail.on_event(8, None, &EventBody::PromptDropped { prompt: 5 });

    assert_eq!(tail.compacting, None);
}

#[test]
fn a_compaction_asked_for_ends_with_the_turn_or_agent_that_takes_it() {
    for over in [turn_ended(), exited(), compacted()] {
        let mut tail = asked_at_5();

        tail.on_event(6, Some(RunId::new(2)), &over);

        assert_eq!(tail.compacting, None, "{over:?}");
    }
}
