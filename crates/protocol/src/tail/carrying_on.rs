//! A worker's tail of each run, and a hub taking it in place of its own.

use super::tests::{started, turn_ended};
use super::*;

fn writing(run: u64, message_id: &str, text: &str) -> StreamDelta {
    StreamDelta {
        run: Some(RunId::new(run)),
        message_id: message_id.into(),
        parent_tool_use_id: None,
        change: DeltaChange::Thinking { index: 0, text: text.into() },
    }
}

/// What a worker has of runs 1 and 2: a message from each, run 1's subagent in the background, and run 2 compacting.
fn two_runs() -> Tail {
    let mut tail = Tail::default();
    tail.apply(&writing(1, "m1", "old"));
    tail.on_agent_event(RunId::new(1), &started("toolu_1", true));
    tail.apply(&writing(2, "m2", "new"));
    tail.on_agent_event(RunId::new(2), &EventBody::AgentCompacting { written_chars: Some(40) });
    tail
}

#[test]
fn a_run_s_tail_holds_only_what_that_run_writes() {
    let tail = two_runs();

    let of_2 = tail.of_run(RunId::new(2));

    assert_eq!(tail.runs(), [RunId::new(1), RunId::new(2)]);
    assert_eq!(of_2.messages.iter().map(|m| m.message_id.as_str()).collect::<Vec<_>>(), ["m2"]);
    assert!(of_2.in_background.is_empty(), "run 1's subagent isn't run 2's");
    assert_eq!(of_2.compacting.map(|c| c.written_chars), Some(Some(40)));
    assert_eq!(tail.of_run(RunId::new(1)).in_background.len(), 1);
    assert!(tail.of_run(RunId::new(1)).compacting.is_none());
}

#[test]
fn replacing_a_run_takes_the_place_of_what_the_hub_had_of_it_and_leaves_the_rest() {
    let mut hub = Tail::default();
    hub.apply(&writing(1, "m1", "kept"));
    hub.apply(&writing(2, "m2", "stale"));
    hub.apply(&writing(2, "m3", "gone"));

    hub.replace_run(RunId::new(2), two_runs().of_run(RunId::new(2)));

    let texts: Vec<_> = hub.messages.iter().map(|m| (m.message_id.as_str(), m.blocks[0].text.as_str())).collect();
    assert_eq!(texts, [("m1", "kept"), ("m2", "new")]);
    assert_eq!(hub.compacting.and_then(|c| c.run), Some(RunId::new(2)));
}

#[test]
fn replacing_a_run_takes_its_subagents_in_the_background_so_its_turn_s_end_leaves_them() {
    let mut worker = Tail::default();
    worker.apply(&StreamDelta { parent_tool_use_id: Some("toolu_1".into()), ..writing(1, "sub", "still going") });
    worker.on_agent_event(RunId::new(1), &started("toolu_1", true));
    let mut hub = Tail::default();

    hub.replace_run(RunId::new(1), worker.of_run(RunId::new(1)));
    hub.on_event(9, Some(RunId::new(1)), &turn_ended());

    assert_eq!(hub.messages.iter().map(|m| m.message_id.as_str()).collect::<Vec<_>>(), ["sub"]);
}

#[test]
fn a_compaction_only_asked_for_stays_unless_the_run_says_it_s_compacting() {
    let mut asked = Tail::default();
    asked.on_event(5, None, &EventBody::CompactionAsked { keeping: None });
    let mut compacting = asked.clone();

    asked.replace_run(RunId::new(1), Tail::default());
    compacting.replace_run(RunId::new(1), two_runs().of_run(RunId::new(2)));

    assert_eq!(asked.compacting.and_then(|c| c.asked), Some(5), "asked of this hub, which the worker can't know");
    let said = compacting.compacting.unwrap();
    assert_eq!((said.run, said.asked, said.written_chars), (Some(RunId::new(1)), None, Some(40)));
}

#[test]
fn replacing_a_run_with_nothing_clears_what_it_was_doing() {
    let mut hub = two_runs();

    hub.replace_run(RunId::new(2), Tail::default());

    assert_eq!(hub.runs(), [RunId::new(1)]);
}
