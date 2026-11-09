//! The screens the gallery draws: what the hub holds for each, from `worlds.rs`, and what to do in the app before
//! taking it.

#![cfg(test)]

use protocol::model::SessionStatus;

use super::world::World;
use super::worlds::{
    attachments, chat_tabs, chat_world, closed_chats, conversation, diff, finishing_update, own_hub, pointed,
    queue_held, queued, setup_failed, streaming, titled, unread,
};
use super::{chats, file_chats, pictures, subagent_chats, tool_chats};

pub(crate) struct Screen {
    /// Also the image's file name.
    pub(crate) name: &'static str,
    /// The spec's file for the area it's about, in `docs/spec/`, which shows it beside the rules it goes with.
    pub(crate) spec: &'static str,
    /// What the hub holds. `None` is no hub at all.
    pub(crate) world: Option<fn() -> World>,
    pub(crate) steps: &'static [Step],
}

/// Something to do in the app once it has connected, each done once the one before it has finished.
pub(crate) enum Step {
    /// Selects the workspace with this name, and waits for its chat, its diff and its pull request to load.
    Select(&'static str),
    /// Clicks this file in the sidebar of changes, and waits for the diff tab to show it.
    OpenFile(&'static str),
    /// Starts a comment on this line of the open file's new side, as the line's add button would.
    StartComment(usize),
    OpenSetupLog,
    /// Presses a button, such as one that opens a menu.
    Click(&'static str),
    /// Moves the pointer somewhere with no hover of its own, so the picture shows what's open rather than what's
    /// under the pointer.
    MoveAway,
    /// Moves the pointer over the tab of the open chat at this position in the workspace showing, and leaves it there.
    HoverTab(usize),
    /// Moves the pointer over the element with this ID, and leaves it there, for what shows only under the pointer.
    Point(&'static str),
    /// Types this where focus is, such as the composer.
    Type(&'static str),
    /// Rests the pointer on an element, and waits for the one with the ID `shows`, such as its tooltip.
    Hover {
        on: Target,
        shows: &'static str,
    },
    /// Pastes an image where focus is, and waits for the composer to take it.
    PasteImage(fn() -> Vec<u8>),
    /// Pastes a copied file with this name and text, and waits for the composer to take it.
    PasteFile(&'static str, &'static str),
    /// Presses Finish now in the update's card, and waits for what it asks.
    AskToFinish,
    /// Right-clicks something by its ID, which opens its menu.
    RightClick(&'static str),
    /// Picks the item with this label in the menu that's open.
    ClickItem(&'static str),
    /// Has the app show this version as downloaded, as if an update had come.
    UpdateReady(&'static str),
    /// Has the app show that it couldn't reach the update server.
    UpdateProblem,
    /// Opens Settings, as its menu item does.
    OpenSettings,
    /// Clicks the transcript's line with this label, such as a tool call, to open it, then moves the pointer away,
    /// so the picture shows it open rather than hovered. The first, if there are several.
    Open(&'static str),
}

/// An element in the app, by its ID, or by the label it gives assistive apps, for one whose ID is made from data
/// the screen can't know, such as a tool call's line.
#[derive(Clone, Copy)]
pub(crate) enum Target {
    Id(&'static str),
    Label(&'static str),
}

/// The workspace each screen is about, in the first repository, below the ones the sidebar always has.
pub(super) const FOCUS: &str = "aspen";

/// A log pasted into the composer, as a file.
const HUB_LOG: &str = "INFO hub: listening\nWARN layout: row fir overflows by 4 px\n";

pub(crate) const SCREENS: &[Screen] = &[
    Screen { name: "no-hub", spec: "updates.md", world: None, steps: &[] },
    Screen { name: "first-run", spec: "workspaces.md", world: Some(World::default), steps: &[] },
    Screen {
        name: "setup-failed",
        spec: "workspaces.md",
        world: Some(setup_failed),
        steps: &[Step::Select(FOCUS), Step::OpenSetupLog],
    },
    Screen { name: "conversation", spec: "chats.md", world: Some(conversation), steps: &[Step::Select(FOCUS)] },
    Screen { name: "streaming", spec: "chats.md", world: Some(streaming), steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "attachments",
        spec: "chats.md",
        world: Some(attachments),
        steps: &[
            Step::Select(FOCUS),
            Step::PasteImage(pictures::terminal),
            Step::PasteFile("hub.log", HUB_LOG),
            Step::Type("And it happens here too"),
        ],
    },
    Screen {
        name: "attachment-opened",
        spec: "chats.md",
        world: Some(attachments),
        steps: &[Step::Select(FOCUS), Step::Click("picture/attachment/shot-1")],
    },
    Screen {
        name: "approval",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, chats::approval())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "background",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Background, chats::background())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "question",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, chats::question())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "steered",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Running, chats::steered())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "queued",
        spec: "chats.md",
        world: Some(queued),
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Id("queued-queued-2"), shows: "edit-queued-queued-2" }],
    },
    Screen { name: "queue-held", spec: "chats.md", world: Some(queue_held), steps: &[Step::Select(FOCUS)] },
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
        name: "tool-subagent",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, subagent_chats::subagents())),
        steps: &[Step::Select(FOCUS), Step::Open("Find checks for a stop signal")],
    },
    Screen {
        name: "tool-web",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, tool_chats::web())),
        steps: &[Step::Select(FOCUS), Step::Open("Search the web"), Step::Open("Fetch")],
    },
    Screen {
        name: "mode-picker",
        spec: "chats.md",
        world: Some(conversation),
        steps: &[Step::Select(FOCUS), Step::Click("mode-picker")],
    },
    Screen {
        name: "chat-tabs",
        spec: "chats.md",
        world: Some(chat_tabs),
        steps: &[Step::Select(FOCUS), Step::HoverTab(1)],
    },
    Screen {
        name: "closed-chats",
        spec: "chats.md",
        world: Some(closed_chats),
        steps: &[Step::Select(FOCUS), Step::Click("closed-chats"), Step::MoveAway],
    },
    Screen { name: "changes", spec: "diff.md", world: Some(diff), steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "changes-hidden",
        spec: "diff.md",
        world: Some(conversation),
        steps: &[Step::Select(FOCUS), Step::Click("toggle-diff")],
    },
    Screen {
        name: "changes-tree",
        spec: "diff.md",
        world: Some(diff),
        steps: &[Step::Select(FOCUS), Step::Click("changes-layout")],
    },
    Screen {
        name: "diff",
        spec: "diff.md",
        world: Some(diff),
        steps: &[Step::Select(FOCUS), Step::OpenFile("crates/protocol/src/model.rs")],
    },
    Screen {
        name: "writing-a-comment",
        spec: "diff.md",
        world: Some(diff),
        steps: &[Step::Select(FOCUS), Step::OpenFile("crates/protocol/src/model.rs"), Step::StartComment(99)],
    },
    Screen { name: "unread", spec: "sidebar.md", world: Some(unread), steps: &[Step::Select(FOCUS)] },
    Screen { name: "home", spec: "home.md", world: Some(titled), steps: &[Step::Select(FOCUS), Step::Click("home")] },
    Screen {
        name: "workspace-menu",
        spec: "sidebar.md",
        world: Some(unread),
        // The screen's own workspace, the fourth, whose other chat has a reply nobody has read.
        steps: &[Step::Select(FOCUS), Step::RightClick("workspace-ws-4")],
    },
    Screen {
        name: "other-workspace-menu",
        spec: "sidebar.md",
        world: Some(unread),
        // The first workspace, which isn't showing, and has a reply nobody has read.
        steps: &[Step::Select(FOCUS), Step::RightClick("workspace-ws-1")],
    },
    Screen {
        name: "chat-menu",
        spec: "sidebar.md",
        world: Some(unread),
        // The tab of the screen's other chat, the seventh made, which has a reply nobody has read.
        steps: &[Step::Select(FOCUS), Step::RightClick("chat-chat-7")],
    },
    Screen {
        name: "archive-button",
        spec: "sidebar.md",
        world: Some(conversation),
        steps: &[Step::Select(FOCUS), Step::Point("workspace-ws-1")],
    },
    Screen {
        name: "rename",
        spec: "workspaces.md",
        world: Some(titled),
        // The screen's own workspace is the fourth.
        steps: &[Step::Select(FOCUS), Step::RightClick("workspace-ws-4"), Step::ClickItem("Rename…")],
    },
    Screen {
        name: "update-ready",
        spec: "updates.md",
        world: Some(conversation),
        steps: &[Step::Select(FOCUS), Step::UpdateReady("1.0.1")],
    },
    Screen {
        name: "restart-to-update-question",
        spec: "updates.md",
        world: Some(conversation),
        steps: &[
            Step::Select(FOCUS),
            Step::Type("Also show it in the header"),
            Step::UpdateReady("1.0.1"),
            Step::Click("restart-to-update"),
        ],
    },
    Screen {
        name: "finishing-update",
        spec: "updates.md",
        world: Some(finishing_update),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "update-details",
        spec: "updates.md",
        world: Some(finishing_update),
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Id("update-details"), shows: "update-details-text" }],
    },
    Screen {
        name: "finish-update-question",
        spec: "updates.md",
        world: Some(finishing_update),
        steps: &[Step::Select(FOCUS), Step::AskToFinish],
    },
    Screen { name: "own-hub", spec: "updates.md", world: Some(own_hub), steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "update-problem",
        spec: "updates.md",
        world: Some(conversation),
        steps: &[Step::Select(FOCUS), Step::UpdateProblem],
    },
    Screen {
        name: "settings",
        spec: "settings.md",
        world: Some(conversation),
        steps: &[Step::Select(FOCUS), Step::OpenSettings],
    },
];
