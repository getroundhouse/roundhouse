//! How a reply of the agent's was heard, read out loud in conversation mode: being read now, cut off where the person
//! talked over it or stopped it, or never read out. The device reading it says how far it's got in the chat's tail,
//! and the hub records how each reply ended. A reply drawn from neither, such as one typed for, is drawn as written.

use std::sync::Arc;

use client_core::session_log::SessionLog;
use gpui_kit::assets::IconName;
use protocol::event::{ContentBlock, EventBody};
use protocol::model::{Ending, MessageId, ReplyHeard, ReplyPlace, Seq, SpokenThrough};

use super::{NEAR, nearby};
use crate::item_key::in_message;
use crate::transcript::Model;

/// How a reply was heard, where it's more than heard to its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::rows) enum Heard {
    /// Being read out, as far as this place.
    Speaking(ReplyPlace),
    /// The person talked over it here.
    TalkedOver(ReplyPlace),
    /// Stop or typing stopped it here.
    Stopped(ReplyPlace),
    /// Never read out, as a reply after one the person talked over isn't.
    NotReadOut,
}

impl Heard {
    /// How the reply `message` was heard, while the device with the conversation is `speaking` some reply, if it is,
    /// with what the hub `recorded` of how it ended, if it has. While the device says it's reading the reply, it's
    /// being spoken, whatever was recorded, so a device whose connection dropped for a moment and goes on reading
    /// isn't shown as stopped.
    pub(in crate::rows) fn of(
        message: &MessageId,
        speaking: Option<&SpokenThrough>,
        recorded: Option<&ReplyHeard>,
    ) -> Option<Self> {
        if let Some(speaking) = speaking.filter(|speaking| &speaking.message == message) {
            return Some(Heard::Speaking(speaking.through));
        }
        match recorded?.ending {
            Ending::TalkedOver { through } => Some(Heard::TalkedOver(through)),
            Ending::Stopped { through } => Some(Heard::Stopped(through)),
            Ending::NotReadOut => Some(Heard::NotReadOut),
            Ending::ToTheEnd | Ending::Unknown => None,
        }
    }

    /// How the reply `message` was heard, as [`Heard::of`] says, from what the frame's `log` and `m` hold.
    pub(in crate::rows) fn in_chat(log: &SessionLog, m: &Model, message: &MessageId) -> Option<Self> {
        Self::of(message, log.tail().speaking.as_ref(), m.heard.get(message).map(Arc::as_ref))
    }

    /// The byte of `text`, the text of the reply's block numbered `block`, from which it wasn't heard, and is dimmed:
    /// none of a block before the place it got to, all of one after it, and from the place on in its own. The place
    /// comes from another build, so it's taken no further than the text's end, and back to a character's start. `None`
    /// where all of it was heard.
    pub(in crate::rows) fn unheard_from(self, block: u32, text: &str) -> Option<usize> {
        let place = match self {
            Heard::Speaking(place) | Heard::TalkedOver(place) | Heard::Stopped(place) => place,
            Heard::NotReadOut => return Some(0),
        };
        match block.cmp(&place.block) {
            std::cmp::Ordering::Less => None,
            std::cmp::Ordering::Greater => Some(0),
            std::cmp::Ordering::Equal => {
                let through = usize::try_from(place.through).unwrap_or(usize::MAX).min(text.len());
                let through = text.floor_char_boundary(through);
                (through < text.len()).then_some(through)
            }
        }
    }

    /// What the reply's line says of it.
    pub(in crate::rows) fn words(self) -> &'static str {
        match self {
            Heard::Speaking(_) => "Speaking",
            Heard::TalkedOver(_) => "Talked over",
            Heard::Stopped(_) => "Stopped",
            Heard::NotReadOut => "Not read out",
        }
    }

    /// The speaker the reply's line shows before what it says: sounding while it's spoken, and struck out otherwise.
    pub(in crate::rows) fn icon(self) -> IconName {
        match self {
            Heard::Speaking(_) => IconName::Volume2,
            Heard::TalkedOver(_) | Heard::Stopped(_) | Heard::NotReadOut => IconName::VolumeX,
        }
    }
}

/// For an agent's own message recorded at `seq`, how the reply it's part of was heard. For a turn's end, how the reply
/// it ended on was, which its line says. `None` for anything else.
pub(super) fn read(log: &SessionLog, m: &Model, seq: Seq, body: &EventBody) -> Option<Heard> {
    let message = if matches!(body, EventBody::TurnEnded { .. }) { ended_on(log, seq)? } else { own_message(body)? };
    Heard::in_chat(log, m, message)
}

/// Where the block at `position` of `body`, which is `block`, wasn't heard, as [`Heard::unheard_from`] says, if it's
/// text, which is all that's read out.
pub(super) fn unheard_in(heard: Heard, body: &EventBody, position: usize, block: &ContentBlock) -> Option<usize> {
    let (EventBody::Assistant { first_index, .. }, ContentBlock::Text { text, .. }) = (body, block) else {
        return None;
    };
    heard.unheard_from(in_message(position, *first_index)?, text)
}

/// The message of the agent's own that `body` is part of, which is what's read out, rather than a subagent's.
fn own_message(body: &EventBody) -> Option<&MessageId> {
    let EventBody::Assistant { message_id, subagent: None, .. } = body else { return None };
    message_id.as_ref()
}

/// The agent's own message the turn ending at `seq` ended on: its last before it, unless the turn before ended first.
fn ended_on(log: &SessionLog, seq: Seq) -> Option<&MessageId> {
    for event in nearby::events_before(log, seq, NEAR) {
        if matches!(event.body, EventBody::TurnEnded { .. }) {
            return None;
        }
        if let EventBody::Assistant { subagent: None, message_id, .. } = &event.body {
            return message_id.as_ref();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};
    use protocol::event::TurnOutcome;
    use protocol::fixtures::turn_ended;
    use protocol::model::{Rev, SettledBy};

    use super::super::Around;
    use crate::fixtures::{event, log, prompt, within};

    fn place(block: u32, through: u32) -> ReplyPlace {
        ReplyPlace { block, through }
    }

    fn ended(message: &str, ending: Ending) -> ReplyHeard {
        ReplyHeard {
            session_id: "chat".into(),
            message: message.into(),
            ending,
            settled_by: SettledBy::Device,
            rev: Rev::new(1),
        }
    }

    #[test]
    fn a_reply_is_heard_as_its_ending_says_and_as_being_spoken_while_the_device_reads_it() {
        let reply = MessageId::from("m");
        let reading = |message: &str| SpokenThrough { message: message.into(), through: place(0, 4) };
        let cut = ended("m", Ending::TalkedOver { through: place(1, 2) });

        let cases = [
            (None, None, None),
            (None, Some(ended("m", Ending::ToTheEnd)), None),
            (None, Some(ended("m", Ending::Unknown)), None),
            (None, Some(cut.clone()), Some(Heard::TalkedOver(place(1, 2)))),
            (None, Some(ended("m", Ending::Stopped { through: place(0, 9) })), Some(Heard::Stopped(place(0, 9)))),
            (None, Some(ended("m", Ending::NotReadOut)), Some(Heard::NotReadOut)),
            (Some(reading("m")), None, Some(Heard::Speaking(place(0, 4)))),
            (Some(reading("m")), Some(cut.clone()), Some(Heard::Speaking(place(0, 4)))),
            (Some(reading("another")), Some(cut), Some(Heard::TalkedOver(place(1, 2)))),
            (Some(reading("another")), None, None),
        ];

        for (speaking, recorded, heard) in cases {
            assert_eq!(Heard::of(&reply, speaking.as_ref(), recorded.as_ref()), heard, "{speaking:?}, {recorded:?}");
        }
    }

    #[test]
    fn what_wasn_t_heard_is_the_rest_of_the_block_it_stopped_in_and_every_block_after() {
        let cut = Heard::TalkedOver(place(1, 6));

        assert_eq!(cut.unheard_from(0, "Sure, on it."), None);
        assert_eq!(cut.unheard_from(1, "Below about two hundred"), Some(6));
        assert_eq!(cut.unheard_from(2, "The name stays readable."), Some(0));
        assert_eq!(cut.unheard_from(1, "Below"), None, "a place past its end is the end, where all of it was heard");
        assert_eq!(Heard::NotReadOut.unheard_from(0, "Sure."), Some(0));
        assert_eq!(Heard::Speaking(place(0, 2)).unheard_from(0, "née"), Some(1), "back to a character's start");
    }

    /// The agent's message `message`, saying each of `texts` in a block of its own, the first numbered `first_index`.
    fn reply(message: &str, first_index: Option<u32>, texts: &[&str]) -> EventBody {
        let text = |text: &&str| ContentBlock::Text { text: (*text).to_string(), images: Vec::new() };
        let blocks = texts.iter().map(text).collect();
        EventBody::Assistant { message_id: Some(message.into()), subagent: None, model: None, blocks, first_index }
    }

    #[test]
    fn a_turn_s_line_says_how_the_reply_it_ended_on_was_heard_and_each_reply_dims_what_wasn_t() {
        let bodies = [
            prompt("Can you sort out the sidebar?"),
            reply("m1", None, &["On it."]),
            within("call-1", reply("sub", None, &["The subagent's own words."])),
            reply("m2", Some(1), &["Keep the badge,", "then drop it."]),
            turn_ended(TurnOutcome::Success),
            prompt("And narrow windows?"),
            turn_ended(TurnOutcome::Success),
        ];
        let log = log(bodies.iter().cloned().zip(1..).map(|(body, seq)| event(Seq::new(seq), body)));
        let mut m = Model::default();
        for heard in [ended("m1", Ending::NotReadOut), ended("m2", Ending::TalkedOver { through: place(2, 5) })] {
            m.heard.insert(heard.message.clone(), Arc::new(heard));
        }
        let around = |seq: u64| Around::read(&log, &m, Seq::new(seq), &bodies[usize::try_from(seq).unwrap() - 1]);
        let unheard = |seq| around(seq).blocks.iter().map(|block| block.unheard_from).collect::<Vec<_>>();

        assert_eq!(
            around(5).heard,
            Some(Heard::TalkedOver(place(2, 5))),
            "the turn's line says how its last was heard"
        );
        assert_eq!(around(7).heard, None, "a turn that ended on no reply of its own says nothing");
        assert_eq!(unheard(4), [None, Some(5)], "blocks numbered from where the deltas numbered the first");
        assert_eq!(unheard(2), [Some(0)], "one never read out is dimmed whole");
        assert_eq!(unheard(3), [None], "a subagent's words aren't read out");
    }

    proptest! {
        #![proptest_config(Config {
            rng_seed: RngSeed::Fixed(1),
            failure_persistence: None,
            ..Config::default()
        })]

        /// A place from another build is taken as far into the text as it can, and never past it or into a character.
        #[test]
        fn any_place_is_somewhere_in_the_text(text in "\\PC{0,40}", through in prop_oneof![0u32..200, any::<u32>()]) {
            if let Some(from) = Heard::Stopped(place(0, through)).unheard_from(0, &text) {
                prop_assert!(from < text.len());
                prop_assert!(text.is_char_boundary(from));
                prop_assert!(usize::try_from(through).unwrap() >= from);
            }
        }
    }
}
