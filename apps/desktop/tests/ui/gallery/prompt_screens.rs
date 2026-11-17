//! Copying prompts of different lengths, with the button over their corner and clear of a code-block reply.

#![cfg(test)]

use protocol::model::{Seq, SessionStatus};

use super::frame::CHAT;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::chat_world;
use client_core::fake_hub::World;
use client_core::fake_hub::events::{replied, taken, text, user_prompt};

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
    Screen {
        name: "copy-prompt-above-code",
        spec: "chats.md",
        world: Some(code_reply),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Point("bubble/event/1/0")],
    },
    Screen {
        name: "copy-code-block",
        spec: "components.md",
        world: Some(code_reply),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Point("code-block-actions")],
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

fn code_reply() -> World {
    let reply = "```python\ndef greet(name):\n    return f\"Hello, {name}!\"\n\nprint(greet(\"Roundhouse\"))\n```";
    chat_world(
        SessionStatus::Idle,
        vec![user_prompt("Show a Python greeting."), taken(Seq::new(1)), text("msg-1", reply), replied(reply, 4, 0.01)],
    )
}
