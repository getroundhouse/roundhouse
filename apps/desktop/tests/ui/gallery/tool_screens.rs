//! The screens of what an agent's tool calls, its questions once they're answered, its thinking and the images it
//! points to show in a chat, apart from the rest in `screens.rs` only to keep each list a readable length.

#![cfg(test)]

use look::Region;
use protocol::model::SessionStatus;

use super::frame::{CHAT, Frame, Part};
use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::worlds::{chat_world, pointed, read_image, thinking_streaming};
use super::{chats, file_chats, question_chats, tool_chats};

pub(crate) const TOOL_SCREENS: &[Screen] = &[
    Screen {
        name: "question-answered",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, question_chats::answered())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "question-answered-opened",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, question_chats::answered())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open(question_chats::TWO_ASKED)],
    },
    Screen {
        name: "tool-commands",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, tool_chats::commands())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open(tool_chats::FORMAT_LINE), Step::Open(tool_chats::SEARCH_LINE)],
    },
    Screen {
        name: "tool-command-whole",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, tool_chats::commands())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open(tool_chats::SEARCH_LINE), Step::Open(tool_chats::SEARCH)],
    },
    Screen {
        name: "finding",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, tool_chats::commands())),
        frame: Frame::Transcript(&[Part::Region(Region::FindBar)]),
        steps: &[Step::Select(FOCUS), Step::Find("fmt")],
    },
    Screen {
        name: "tool-any",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, tool_chats::any_tool())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Create issue")],
    },
    Screen {
        name: "tool-files",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, file_chats::files())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Edit")],
    },
    Screen {
        name: "tool-edit-preview",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, file_chats::files())),
        // The preview reaches past the chat, over the sidebar of changes.
        frame: Frame::Around(Part::Line("Edit"), &[Part::Region(Region::EditPreview)]),
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Label("Edit"), shows: Region::EditPreview.id() }],
    },
    Screen {
        name: "tool-file-contents",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, file_chats::file_contents())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Read 10 lines"), Step::Open("Write 12 lines")],
    },
    Screen {
        name: "tool-read-image",
        spec: "chats.md",
        world: Some(read_image),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Read 1 image")],
    },
    Screen {
        name: "tool-search",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, file_chats::searches())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Search")],
    },
    Screen {
        name: "tool-failures",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, file_chats::failures())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Edit"), Step::Open("Search")],
    },
    Screen { name: "reply-image", spec: "chats.md", world: Some(pointed), frame: CHAT, steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "thinking",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, chats::thinking())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Thinking")],
    },
    Screen {
        name: "thinking-streaming",
        spec: "chats.md",
        world: Some(thinking_streaming),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "tool-web",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, tool_chats::web())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Search the web"), Step::Open("Fetch")],
    },
    Screen {
        name: "tool-advisor",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, tool_chats::advisor())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Ask the advisor")],
    },
];
