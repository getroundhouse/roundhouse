//! Clips of finding text in a chat.

#![cfg(test)]

use client_core::fake_hub::World;
use client_core::fake_hub::events::{text, turn, user_prompt};
use protocol::model::SessionStatus;

use super::folder::record_into_its_folder;
use crate::gallery::screens::FOCUS;
use crate::gallery::steps::{Clip, Frames, Shot, Step};
use crate::gallery::worlds::chat_world;

/// Find goes on to a match far down a long reply, and `Option-Up` is pressed in the next frame, while the chat is
/// still on its way there. The chat goes where `Option-Up` sends it, since the step takes over from bringing the
/// match into view.
const A_STEP_AS_FIND_GOES_TO_A_MATCH: Clip = Clip {
    name: "a-step-as-find-goes-to-a-match",
    world: a_reply_with_a_match_at_each_end,
    steps: &[Step::Select(FOCUS), Step::Find("needle")],
    shots: &[
        Shot::Hold(Frames(30)),
        Shot::Press("enter"),
        Shot::Hold(Frames(1)),
        Shot::Press("alt-up"),
        Shot::Hold(Frames(60)),
    ],
};

#[test]
#[ignore = "records a clip, to watch"]
fn a_step_as_find_goes_to_a_match() {
    record_into_its_folder(&A_STEP_AS_FIND_GOES_TO_A_MATCH);
}

/// A chat of two turns: a long reply, and a longer one with `needle` at its start and at its end.
pub(super) fn a_reply_with_a_match_at_each_end() -> World {
    let paragraphs = |word: &str| Vec::from_fn(60, |n| format!("{word} {}.", n + 1)).join("\n\n");
    let mut events = Vec::new();
    turn(&mut events, user_prompt("What came first?"), [text("early", &paragraphs("Early"))]);
    let both = format!("A needle here.\n\n{}\n\nA needle there.", paragraphs("Part"));
    turn(&mut events, user_prompt("Where are they?"), [text("both", &both)]);
    chat_world(SessionStatus::Idle, events)
}
