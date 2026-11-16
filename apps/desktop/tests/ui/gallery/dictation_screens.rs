//! The screens of dictation in the composer, each of a state its rules in `chats.md` say, apart from the rest in
//! `screens.rs` only to keep each list a readable length.

#![cfg(test)]

use desktop::dictating::STATUS;
use desktop::workspace_pane::{MICROPHONE, TURN_ON};

use super::frame::{COMPOSER, DICTATION_CARD, Frame, QUESTION};
use super::screens::FOCUS;
use super::steps::{DictationTarget, Screen, Step};
use super::worlds::{chat_world, conversation};
use super::{chats, question_chats};
use protocol::model::SessionStatus;

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "dictation",
        spec: "chats.md",
        world: Some(conversation),
        frame: COMPOSER,
        steps: &[
            Step::Select(FOCUS),
            Step::Type("In the sidebar,"),
            Step::Dictate {
                into: DictationTarget::Composer,
                said: "let the name shrink and keep the badge",
                shows: "let the name shrink and keep the",
            },
            Step::MoveAway,
        ],
    },
    Screen {
        name: "new-workspace-dictation",
        spec: "workspaces.md",
        world: Some(conversation),
        frame: Frame::Dialog,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("new-in-repo-1"),
            Step::Type("In the sidebar,"),
            Step::Dictate {
                into: DictationTarget::NewWorkspace,
                said: "let the name shrink and keep the badge",
                shows: "let the name shrink and keep the",
            },
            Step::MoveAway,
        ],
    },
    Screen {
        name: "question-dictation",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, question_chats::question())),
        frame: QUESTION,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("question-other"),
            Step::Type("Keep it in the hub."),
            Step::Dictate {
                into: DictationTarget::Answer,
                said: "let every client read the same state",
                shows: "let every client read the same",
            },
            Step::MoveAway,
        ],
    },
    Screen {
        name: "questions-dictation",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, question_chats::questions())),
        frame: QUESTION,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("question-dot-1"),
            Step::Click("question-answer-0"),
            Step::Click("question-other"),
            Step::Dictate {
                into: DictationTarget::Answer,
                said: "show merged until the workspace is archived",
                shows: "show merged until the workspace is",
            },
            Step::MoveAway,
        ],
    },
    Screen {
        name: "approval-dictation",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, chats::approval())),
        frame: QUESTION,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("question-other"),
            Step::Dictate {
                into: DictationTarget::Answer,
                said: "read how the sidebar gets its records first",
                shows: "read how the sidebar gets its records",
            },
            Step::MoveAway,
        ],
    },
    Screen {
        name: "written-answer-dictation",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, question_chats::written_answer())),
        frame: QUESTION,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("question-other"),
            Step::Dictate {
                into: DictationTarget::Answer,
                said: "keep the last state until the hub returns",
                shows: "keep the last state until the hub",
            },
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
