//! What has to hold for any stream of deltas, whatever order they arrive in.

use std::collections::BTreeMap;

use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, RngSeed};

use super::tests::{started, turn_ended};
use super::*;

fn kind() -> impl Strategy<Value = BlockKind> {
    prop_oneof![Just(BlockKind::Text), Just(BlockKind::Thinking), Just(BlockKind::ToolUse), Just(BlockKind::Unknown)]
}

/// Few message IDs and block indexes, so that deltas land on the same blocks often.
fn delta() -> impl Strategy<Value = StreamDelta> {
    let change = prop_oneof![
        (0u32..3, kind(), option::of("[a-z]{1,4}")).prop_map(|(index, kind, tool_name)| DeltaChange::BlockStart {
            index,
            kind,
            tool_name
        }),
        (0u32..3, ".{0,6}").prop_map(|(index, text)| DeltaChange::Text { index, text }),
        (0u32..3, ".{0,6}").prop_map(|(index, text)| DeltaChange::Thinking { index, text }),
        (0u32..3, ".{0,6}").prop_map(|(index, json)| DeltaChange::ToolInput { index, json }),
        Just(DeltaChange::MessageStop),
        Just(DeltaChange::Unknown),
    ];
    (option::of(run()), "m[0-2]", option::of("t[01]"), change).prop_map(
        |(run, message_id, parent_tool_use_id, change)| StreamDelta {
            run,
            message_id: message_id.into(),
            parent_tool_use_id: parent_tool_use_id.map(Into::into),
            change,
        },
    )
}

/// Three runs, so one often ends while another is writing, earlier or later.
fn run() -> impl Strategy<Value = RunId> {
    (1u64..4).prop_map(RunId::new)
}

/// What one run's agent says that changes the tail.
#[derive(Clone, Debug)]
enum Heard {
    Delta(StreamDelta),
    Event(EventBody),
}

/// A delta, or a message finishing, the turn ending, or a subagent starting in the background, on the same few
/// messages and calls as [`delta`].
fn heard() -> impl Strategy<Value = Heard> {
    prop_oneof![
        4 => delta().prop_map(Heard::Delta),
        1 => "m[0-2]".prop_map(|message_id| Heard::Event(finished(&message_id.into()))),
        1 => Just(Heard::Event(turn_ended())),
        1 => "t[01]".prop_map(|call| Heard::Event(started(&call, true))),
    ]
}

/// What a hub has of `run` once it has heard `heard`, each event recorded as it came.
fn heard_by_hub(run: RunId, heard: &[Heard]) -> Tail {
    let mut tail = Tail::default();
    hear(&mut tail, run, heard);
    tail
}

fn hear(hub: &mut Tail, run: RunId, heard: &[Heard]) {
    for (seq, heard) in (1..).zip(heard) {
        match heard {
            Heard::Delta(delta) => hub.apply(&StreamDelta { run: Some(run), ..delta.clone() }),
            Heard::Event(body) => hub.on_event(Seq::new(seq), Some(run), body),
        }
    }
}

fn applied(deltas: &[StreamDelta]) -> Tail {
    let mut tail = Tail::default();
    for delta in deltas {
        tail.apply(delta);
    }
    tail
}

fn finished(message_id: &MessageId) -> EventBody {
    EventBody::Assistant {
        message_id: Some(message_id.clone()),
        parent_tool_use_id: None,
        model: None,
        blocks: Vec::new(),
        first_index: None,
    }
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// The reason the type exists: a client attaching mid-message gets a snapshot over the wire, then
    /// the deltas after it, and ends up with exactly what the hub has.
    #[test]
    fn attaching_at_any_point_ends_with_what_the_hub_has(deltas in vec(delta(), 0..40), at in any::<Index>()) {
        let at = at.index(deltas.len() + 1);
        let snapshot = serde_json::to_string(&applied(&deltas[..at])).unwrap();

        let mut client: Tail = serde_json::from_str(&snapshot).unwrap();
        for delta in &deltas[at..] {
            client.apply(delta);
        }

        prop_assert_eq!(client, applied(&deltas));
    }

    /// Why a worker keeps what its agent is writing: a hub that connects to it partway through a run, whatever it held
    /// of the run already, takes the worker's tail of it, over the wire, then hears the rest, and ends up with what a
    /// hub that heard it all has.
    #[test]
    fn taking_a_run_s_tail_at_any_point_ends_with_what_a_hub_that_heard_it_all_has(
        run in run(),
        heard in vec(heard(), 0..40),
        at in any::<Index>(),
        held in any::<Index>(),
    ) {
        let at = at.index(heard.len() + 1);
        let held = held.index(at + 1);
        let mut worker = Tail::default();
        for heard in &heard[..at] {
            match heard {
                Heard::Delta(delta) => worker.apply(&StreamDelta { run: Some(run), ..delta.clone() }),
                Heard::Event(body) => worker.on_agent_event(run, body),
            }
        }
        let sent = serde_json::to_string(&worker.of_run(run)).unwrap();

        let mut hub = heard_by_hub(run, &heard[..held]);
        hub.replace_run(run, serde_json::from_str(&sent).unwrap());
        hear(&mut hub, run, &heard[at..]);

        prop_assert_eq!(hub, heard_by_hub(run, &heard));
    }

    /// A block's text is everything sent to it, in order, since its message last stopped.
    #[test]
    fn a_block_holds_everything_sent_to_it_in_order(deltas in vec(delta(), 0..40)) {
        let mut expected: BTreeMap<(MessageId, u32), String> = BTreeMap::new();
        for delta in &deltas {
            match &delta.change {
                DeltaChange::Text { index, text }
                | DeltaChange::Thinking { index, text }
                | DeltaChange::ToolInput { index, json: text } => {
                    expected.entry((delta.message_id.clone(), *index)).or_default().push_str(text);
                }
                DeltaChange::BlockStart { index, .. } => {
                    expected.entry((delta.message_id.clone(), *index)).or_default();
                }
                DeltaChange::MessageStop => expected.retain(|(message, _), _| *message != delta.message_id),
                DeltaChange::Unknown => {}
            }
        }

        let tail = applied(&deltas);

        let held: BTreeMap<(MessageId, u32), String> = tail
            .messages
            .iter()
            .flat_map(|m| m.blocks.iter().map(|b| ((m.message_id.clone(), b.index), b.text.clone())))
            .collect();
        prop_assert_eq!(held, expected);
    }

    /// A message and its blocks appear once, however many deltas arrive for them.
    #[test]
    fn messages_and_blocks_are_never_duplicated(deltas in vec(delta(), 0..40)) {
        let tail = applied(&deltas);

        let mut ids: Vec<_> = tail.messages.iter().map(|m| &m.message_id).collect();
        let count = ids.len();
        ids.sort();
        ids.dedup();
        prop_assert_eq!(ids.len(), count);
        for message in &tail.messages {
            let mut indexes: Vec<_> = message.blocks.iter().map(|b| b.index).collect();
            let count = indexes.len();
            indexes.sort_unstable();
            indexes.dedup();
            prop_assert_eq!(indexes.len(), count);
        }
    }

    /// A finished message leaves the tail, and takes nothing else with it.
    #[test]
    fn a_finished_message_only_removes_itself(deltas in vec(delta(), 0..40), which in "m[0-2]") {
        let before = applied(&deltas);
        let mut after = before.clone();

        after.on_event(Seq::new(1), None, &finished(&which.as_str().into()));

        let others: Vec<_> = before.messages.iter().filter(|m| m.message_id.as_str() != which).cloned().collect();
        prop_assert_eq!(after.messages, others);
    }

    /// The end of a run's turn clears what that run or an earlier one left open, and leaves a later
    /// run's writing alone, however late it arrives.
    #[test]
    fn the_end_of_a_turn_never_clears_a_later_run(deltas in vec(delta(), 0..40), ended in run()) {
        let before = applied(&deltas);
        let mut after = before.clone();

        after.on_event(Seq::new(1), Some(ended), &turn_ended());

        let later: Vec<_> = before.messages.iter().filter(|m| m.run.is_some_and(|run| run > ended)).cloned().collect();
        prop_assert_eq!(after.messages, later);
    }
}
