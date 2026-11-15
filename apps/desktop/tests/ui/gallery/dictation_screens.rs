//! The screens of dictation in the composer, each of a state its rules in `chats.md` say, apart from the rest in
//! `screens.rs` only to keep each list a readable length.

#![cfg(test)]

use desktop::dictating::STATUS;
use desktop::workspace_pane::{MICROPHONE, TURN_ON};

use super::frame::{COMPOSER, DICTATION_CARD};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::conversation;

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "dictation",
        spec: "chats.md",
        world: Some(conversation),
        frame: COMPOSER,
        steps: &[
            Step::Select(FOCUS),
            Step::Type("In the sidebar,"),
            Step::Dictate { said: "let the name shrink and keep the badge", shows: "let the name shrink and keep the" },
            Step::MoveAway,
        ],
    },
    Screen {
        name: "dictation-offer",
        spec: "chats.md",
        world: Some(conversation),
        frame: DICTATION_CARD,
        steps: &[Step::Select(FOCUS), Step::Click(MICROPHONE), Step::WaitFor(TURN_ON), Step::MoveAway],
    },
    Screen {
        name: "dictation-not-ready",
        spec: "chats.md",
        world: Some(conversation),
        frame: DICTATION_CARD,
        steps: &[
            Step::Select(FOCUS),
            Step::StallDictationDownload,
            Step::OpenSettings,
            Step::Click("settings-dictation"),
            Step::ClickItem("On"),
            Step::WaitFor(STATUS),
            Step::Press("escape"),
            Step::Click(MICROPHONE),
            Step::WaitFor(STATUS),
            Step::MoveAway,
        ],
    },
];
