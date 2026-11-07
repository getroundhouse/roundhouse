//! What has to hold for any stream of deltas, whatever order they arrive in.

use std::collections::BTreeMap;

use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, RngSeed};

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
    ("m[0-2]", option::of("t[01]"), change).prop_map(|(message_id, parent_tool_use_id, change)| StreamDelta {
        message_id: message_id.into(),
        parent_tool_use_id: parent_tool_use_id.map(Into::into),
        change,
    })
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

        after.on_event(&finished(&which.as_str().into()));

        let others: Vec<_> = before.messages.iter().filter(|m| m.message_id.as_str() != which).cloned().collect();
        prop_assert_eq!(after.messages, others);
    }
}
