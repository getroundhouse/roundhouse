//! The screens of what an agent's tool calls, its thinking and the images it points to show in a chat, apart from
//! the rest in `screens.rs` only to keep each list a readable length.

#![cfg(test)]

use protocol::model::SessionStatus;

use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::worlds::{chat_world, pointed, read_image, thinking_streaming};
use super::{chats, file_chats, tool_chats};

pub(crate) const TOOL_SCREENS: &[Screen] = &[
    Screen {
        name: "tool-commands",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, tool_chats::commands())),
        steps: &[Step::Select(FOCUS), Step::Open("Run the signal tests")],
    },
    Screen {
        name: "tool-any",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, tool_chats::any_tool())),
        steps: &[Step::Select(FOCUS), Step::Open("Create issue")],
    },
    Screen {
        name: "tool-files",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, file_chats::files())),
        steps: &[Step::Select(FOCUS), Step::Open("Edit")],
    },
    Screen {
        name: "tool-edit-preview",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, file_chats::files())),
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Label("Edit"), shows: "edit-preview" }],
    },
    Screen {
        name: "tool-file-contents",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, file_chats::file_contents())),
        steps: &[Step::Select(FOCUS), Step::Open("Read 10 lines"), Step::Open("Write 12 lines")],
    },
    Screen {
        name: "tool-read-image",
        spec: "chats.md",
        world: Some(read_image),
        steps: &[Step::Select(FOCUS), Step::Open("Read 1 image")],
    },
    Screen {
        name: "tool-search",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, file_chats::searches())),
        steps: &[Step::Select(FOCUS), Step::Open("Search")],
    },
    Screen {
        name: "tool-failures",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, file_chats::failures())),
        steps: &[Step::Select(FOCUS), Step::Open("Edit"), Step::Open("Search")],
    },
    Screen { name: "reply-image", spec: "chats.md", world: Some(pointed), steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "thinking",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, chats::thinking())),
        steps: &[Step::Select(FOCUS), Step::Open("Thinking")],
    },
    Screen {
        name: "thinking-streaming",
        spec: "chats.md",
        world: Some(thinking_streaming),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "tool-web",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, tool_chats::web())),
        steps: &[Step::Select(FOCUS), Step::Open("Search the web"), Step::Open("Fetch")],
    },
    Screen {
        name: "tool-advisor",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, tool_chats::advisor())),
        steps: &[Step::Select(FOCUS), Step::Open("Ask the advisor")],
    },
];
