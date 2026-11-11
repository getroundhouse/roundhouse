use super::*;
use crate::event::{ExitReason, SubagentOutcome};

fn delta(change: DeltaChange) -> StreamDelta {
    StreamDelta { run: None, message_id: "m1".into(), parent_tool_use_id: None, change }
}

fn assistant(id: &str) -> EventBody {
    EventBody::Assistant {
        message_id: Some(id.into()),
        parent_tool_use_id: None,
        model: None,
        blocks: vec![],
        first_index: None,
    }
}

#[test]
fn deltas_build_up_text_even_without_a_block_start() {
    let mut tail = Tail::default();
    tail.apply(&delta(DeltaChange::Text { index: 0, text: "hel".into() }));
    tail.apply(&delta(DeltaChange::Text { index: 0, text: "lo".into() }));
    assert_eq!(tail.messages[0].blocks[0].text, "hello");
    assert_eq!(tail.messages[0].blocks[0].kind, BlockKind::Text);
}

#[test]
fn a_finished_block_leaves_the_tail_and_the_next_block_starts_clean() {
    let mut tail = Tail::default();
    tail.apply(&delta(DeltaChange::Text { index: 0, text: "first".into() }));
    tail.on_event(1, None, &assistant("m1"));
    assert!(tail.is_empty());
    tail.apply(&delta(DeltaChange::BlockStart { index: 1, kind: BlockKind::ToolUse, tool_name: Some("Bash".into()) }));
    tail.apply(&delta(DeltaChange::ToolInput { index: 1, json: "{\"c".into() }));
    let block = &tail.messages[0].blocks[0];
    assert_eq!((block.index, block.kind, block.text.as_str()), (1, BlockKind::ToolUse, "{\"c"));
}

#[test]
fn messages_stay_separate_and_only_the_named_one_is_cleared() {
    let mut tail = Tail::default();
    tail.apply(&delta(DeltaChange::Text { index: 0, text: "main".into() }));
    tail.apply(&StreamDelta {
        run: None,
        message_id: "sub".into(),
        parent_tool_use_id: Some("toolu_1".into()),
        change: DeltaChange::Text { index: 0, text: "sub".into() },
    });
    assert_eq!(tail.messages.len(), 2);
    tail.on_event(1, None, &assistant("sub"));
    assert_eq!(tail.messages.len(), 1);
    assert_eq!(tail.messages[0].message_id, "m1".into());
}

#[test]
fn the_tail_of_the_agent_or_a_subagent_is_only_what_that_one_writes() {
    let mut tail = Tail::default();
    tail.apply(&delta(DeltaChange::Text { index: 0, text: "own".into() }));
    for (message, call) in [("a1", "toolu_a"), ("b1", "toolu_b"), ("a2", "toolu_a")] {
        let change = DeltaChange::Text { index: 0, text: message.into() };
        tail.apply(&StreamDelta {
            run: None,
            message_id: message.into(),
            parent_tool_use_id: Some(call.into()),
            change,
        });
    }
    let ids = |tail: Tail| tail.messages.into_iter().map(|m| m.message_id).collect::<Vec<_>>();
    assert_eq!(ids(tail.of(None)), vec!["m1".into()]);
    assert_eq!(ids(tail.of(Some(&"toolu_a".into()))), vec![MessageId::from("a1"), "a2".into()]);
    assert_eq!(ids(tail.of(Some(&"toolu_b".into()))), vec![MessageId::from("b1")]);
    assert!(tail.of(Some(&"toolu_c".into())).is_empty());
}

fn turn_ended() -> EventBody {
    EventBody::TurnEnded {
        outcome: crate::event::TurnOutcome::Cancelled,
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

fn started(call: &str, in_background: bool) -> EventBody {
    EventBody::SubagentStarted { call: call.into(), kind: None, description: None, prompt: None, in_background }
}

/// Text that `call`'s subagent writes, in its message `message`.
fn writes(tail: &mut Tail, call: &str, message: &str, text: &str) {
    let change = DeltaChange::Text { index: 0, text: text.into() };
    let run = Some(RunId::new(1));
    tail.apply(&StreamDelta { run, message_id: message.into(), parent_tool_use_id: Some(call.into()), change });
}

fn text(tail: &Tail) -> String {
    tail.messages.iter().flat_map(|m| &m.blocks).map(|b| b.text.as_str()).collect()
}

#[test]
fn the_end_of_a_turn_clears_what_the_agent_was_writing() {
    let mut tail = Tail::default();
    tail.apply(&delta(DeltaChange::Text { index: 0, text: "cut off".into() }));
    tail.on_event(1, None, &turn_ended());
    assert!(tail.is_empty());
}

#[test]
fn the_end_of_a_turn_leaves_what_a_subagent_in_the_background_writes_and_clears_one_in_the_foreground() {
    let (mut tail, run) = (Tail::default(), Some(RunId::new(1)));
    tail.on_event(1, run, &started("toolu_bg", true));
    tail.on_event(1, run, &started("toolu_fg", false));
    writes(&mut tail, "toolu_bg", "bg", "There's ");
    writes(&mut tail, "toolu_fg", "fg", "Half");

    tail.on_event(1, run, &turn_ended());
    writes(&mut tail, "toolu_bg", "bg", "a README ");

    assert_eq!(text(&tail.of(Some(&"toolu_bg".into()))), "There's a README ");
    assert!(tail.of(Some(&"toolu_fg".into())).is_empty());
}

#[test]
fn a_subagents_end_clears_what_it_was_writing_and_the_agents_end_clears_the_rest() {
    let (mut tail, run) = (Tail::default(), Some(RunId::new(1)));
    tail.on_event(1, run, &started("toolu_a", true));
    tail.on_event(1, run, &started("toolu_b", true));
    writes(&mut tail, "toolu_a", "a", "Cut off");
    writes(&mut tail, "toolu_b", "b", "Going on");

    tail.on_event(
        1,
        run,
        &EventBody::SubagentEnded { call: "toolu_a".into(), outcome: SubagentOutcome::Stopped, report: None },
    );
    assert_eq!(text(&tail), "Going on");
    writes(&mut tail, "toolu_a", "a2", "Late");
    tail.on_event(1, run, &turn_ended());
    assert_eq!(text(&tail), "Going on", "an ended subagent is no longer in the background");

    let exited =
        EventBody::AgentExited { reason: ExitReason::Requested, code: Some(0), signal: None, stderr_tail: vec![] };
    tail.on_event(1, run, &exited);
    assert_eq!(tail, Tail::default(), "nothing is left, nor which subagents were in the background");
}
