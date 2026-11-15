//! Copying prompts of different lengths, with the button below their shared right edge.

#![cfg(test)]

use protocol::model::{Seq, SessionStatus};

use super::events::{replied, taken, text, user_prompt};
use super::frame::CHAT;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::world::World;
use super::worlds::chat_world;

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "copy-short-prompt",
        spec: "chats.md",
        world: Some(prompts),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Point("bubble/event/1/0")],
    },
    Screen {
        name: "copy-long-prompt",
        spec: "chats.md",
        world: Some(prompts),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Point("bubble/event/5/0")],
    },
];

fn prompts() -> World {
    let first = "I'll keep the badge at its own width.";
    let second = "The name now shrinks to fit, and the badge stays whole.";
    chat_world(
        SessionStatus::Idle,
        vec![
            user_prompt("do that"),
            taken(Seq::new(1)),
            text("msg-1", first),
            replied(first, 4, 0.01),
            user_prompt(
                "When the sidebar is narrow, let the workspace's name shrink and end in an ellipsis. Keep the pull \
                 request badge at its own width, with enough room between it and the name to read both clearly.",
            ),
            taken(Seq::new(5)),
            text("msg-2", second),
            replied(second, 12, 0.04),
        ],
    )
}
