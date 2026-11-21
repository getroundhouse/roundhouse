//! Clips of reading and writing in a chat.

#![cfg(test)]

use gpui_kit::px;

use super::folder::record_into_its_folder;
use crate::gallery::screens::FOCUS;
use crate::gallery::steps::{Clip, Frames, Shot, Step, Target};

/// Scrolls up a long chat, goes back to its end with the button for it, and writes a prompt, which has the clip scroll,
/// point, click and type, as a person does.
const BACK_TO_THE_END_TO_WRITE: Clip = Clip {
    name: "back-to-the-end-to-write",
    world: super::finding::a_reply_with_a_match_at_each_end,
    steps: &[Step::Select(FOCUS)],
    shots: &[
        Shot::Hold(Frames(20)),
        Shot::Scroll { over: Target::Id("transcript"), down: px(-1200.), frames: Frames(30) },
        Shot::Hold(Frames(30)),
        Shot::Point(Target::Id("latest-button")),
        Shot::Hold(Frames(20)),
        Shot::Click(Target::Id("latest-button")),
        Shot::Hold(Frames(40)),
        Shot::Click(Target::Id("composer")),
        Shot::Hold(Frames(10)),
        Shot::Type("Thanks, that's it."),
        Shot::Hold(Frames(30)),
    ],
};

#[test]
#[ignore = "records a clip, to watch"]
fn back_to_the_end_to_write() {
    record_into_its_folder(&BACK_TO_THE_END_TO_WRITE);
}
