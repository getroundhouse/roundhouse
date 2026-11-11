//! The screens the gallery draws: what the hub holds for each, from `worlds.rs`, and what to do in the app before
//! taking it, in the steps `steps.rs` has.

#![cfg(test)]

use look::Region;
use protocol::model::SessionStatus;

use super::frame::{
    CHAT, COMPOSER, COMPOSER_MENU, CONTEXT, Frame, OUTLINE, Part, QUESTION, QUEUE, SIDEBAR, SIDEBAR_MENU, TABS,
    TABS_MENU, TURN_CARD,
};
use super::steps::{Screen, Step, Target};
use super::world::World;
use super::worlds::{
    approval, attachments, chat_tabs, chat_world, closed_chats, context, context_without_limits, conversation,
    planning, queue_held, queued, setup_failed, steered, streaming, titled, unread, unread_titled, working_background,
};
use super::{chats, long_chats, pictures, question_chats};

/// The workspace each screen is about, in the first repository, below the ones the sidebar always has.
pub(super) const FOCUS: &str = "aspen";

/// A task described in the new-workspace dialog.
const NEW_WORKSPACE_PROMPT: &str = "When the sidebar is narrow, the PR badge runs over the workspace's name. Let the name \
                                    shrink and end in an ellipsis, and keep the badge at its own width.";

/// A log pasted into the composer, as a file.
const HUB_LOG: &str = "INFO hub: listening\nWARN layout: row fir overflows by 4 px\n";

pub(crate) const SCREENS: &[Screen] = &[
    Screen { name: "first-run", spec: "workspaces.md", world: Some(World::default), frame: Frame::Layout, steps: &[] },
    Screen {
        name: "setup-failed",
        spec: "workspaces.md",
        world: Some(setup_failed),
        frame: Frame::Around(Part::Region(Region::SetupLog), &[Part::Region(Region::Composer)]),
        steps: &[Step::Select(FOCUS), Step::OpenSetupLog],
    },
    Screen {
        name: "conversation",
        spec: "chats.md",
        world: Some(conversation),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "turn-card",
        spec: "chats.md",
        world: Some(conversation),
        frame: TURN_CARD,
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id(chats::FINISHED_TURN_END), shows: Region::TurnCard.id() },
        ],
    },
    Screen {
        name: "turn-card-midnight",
        spec: "chats.md",
        world: Some(|| conversation().last_turn_past_midnight()),
        frame: TURN_CARD,
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id(chats::FINISHED_TURN_END), shows: Region::TurnCard.id() },
        ],
    },
    Screen { name: "streaming", spec: "chats.md", world: Some(streaming), frame: CHAT, steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "context",
        spec: "chats.md",
        world: Some(context),
        frame: CONTEXT,
        // The bars come once the hub has said what's left of the limits, which the picture is taken with.
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("context"), shows: Region::ContextCard.id() },
        ],
    },
    Screen {
        name: "context-account",
        spec: "chats.md",
        world: Some(context_without_limits),
        // What the hover waits for is the bottom of the context card, so the frame names the whole card.
        frame: CONTEXT,
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Id("context"), shows: "account" }],
    },
    Screen {
        name: "usage",
        spec: "chats.md",
        world: Some(context),
        frame: Frame::Around(Part::Region(Region::Composer), &[Part::Region(Region::UsageCard)]),
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("usage"), shows: Region::UsageCard.id() },
        ],
    },
    Screen {
        name: "attachments",
        spec: "chats.md",
        world: Some(attachments),
        // The prompt that carried them, and the files waiting in the composer.
        frame: Frame::Transcript(&[Part::Region(Region::Composer)]),
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
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Click("picture/attachment/shot-1")],
    },
    Screen { name: "approval", spec: "chats.md", world: Some(approval), frame: CHAT, steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "background",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Background, chats::background())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "working-background",
        spec: "chats.md",
        world: Some(working_background),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "question",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, question_chats::question())),
        frame: QUESTION,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "questions",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, question_chats::questions())),
        frame: QUESTION,
        steps: &[
            Step::Select(FOCUS),
            // Answers the first, which brings up the second.
            Step::Click("question-answer-0"),
            Step::Click("question-answer-0"),
            Step::Click("question-answer-1"),
            Step::Click("question-other"),
            Step::Type("Only while it's open"),
        ],
    },
    Screen { name: "steered", spec: "chats.md", world: Some(steered), frame: CHAT, steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "queued",
        spec: "chats.md",
        world: Some(queued),
        frame: QUEUE,
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Id("queued-queued-2"), shows: "edit-queued-queued-2" }],
    },
    Screen {
        name: "queue-held",
        spec: "chats.md",
        world: Some(queue_held),
        frame: QUEUE,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "outline",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, long_chats::many_turns())),
        frame: OUTLINE,
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id(Region::Outline.id()), shows: "outline-list" },
            Step::Click(long_chats::FOURTEENTH_PROMPT),
            Step::MoveAway,
            Step::LetTheScrollbarFade,
        ],
    },
    Screen {
        name: "outline-list",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, long_chats::many_turns())),
        frame: OUTLINE,
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id(Region::Outline.id()), shows: "outline-list" },
            Step::Click(long_chats::FOURTEENTH_PROMPT),
            Step::MoveAway,
            Step::Hover { on: Target::Id(Region::Outline.id()), shows: "outline-list" },
            Step::LetTheScrollbarFade,
        ],
    },
    Screen {
        name: "plan-mode",
        spec: "chats.md",
        world: Some(planning),
        frame: COMPOSER,
        steps: &[Step::Select(FOCUS), Step::Type("Plan how a workspace's row shows its pull request")],
    },
    Screen {
        name: "model-picker",
        spec: "chats.md",
        world: Some(conversation),
        frame: COMPOSER_MENU,
        steps: &[Step::Select(FOCUS), Step::Click("model-picker")],
    },
    Screen {
        name: "composer-menu",
        spec: "chats.md",
        world: Some(conversation),
        frame: COMPOSER_MENU,
        steps: &[Step::Select(FOCUS), Step::Click("add")],
    },
    Screen {
        name: "composer-menu-planning",
        spec: "chats.md",
        world: Some(planning),
        frame: COMPOSER_MENU,
        steps: &[Step::Select(FOCUS), Step::Click("add")],
    },
    Screen {
        name: "chat-tabs",
        spec: "chats.md",
        world: Some(chat_tabs),
        frame: TABS,
        steps: &[Step::Select(FOCUS), Step::HoverTab(1)],
    },
    Screen {
        name: "closed-chats",
        spec: "chats.md",
        world: Some(closed_chats),
        frame: TABS_MENU,
        steps: &[Step::Select(FOCUS), Step::Click("closed-chats"), Step::MoveAway],
    },
    Screen { name: "unread", spec: "sidebar.md", world: Some(unread), frame: SIDEBAR, steps: &[Step::Select(FOCUS)] },
    Screen { name: "unread-tab", spec: "sidebar.md", world: Some(unread), frame: TABS, steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "sidebar-hidden",
        spec: "sidebar.md",
        world: Some(conversation),
        // The pointer is left on the workspace's name, which has nothing to show for it, rather than on the button.
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::Click("toggle-sidebar"), Step::Point("workspace-title")],
    },
    Screen {
        name: "home",
        spec: "home.md",
        world: Some(titled),
        frame: Frame::Exactly(Part::Region(Region::Home), &[]),
        steps: &[Step::Select(FOCUS), Step::Click("home")],
    },
    Screen {
        name: "workspace-menu",
        spec: "sidebar.md",
        world: Some(unread_titled),
        // The screen's own workspace, the fourth, whose other chat has a reply nobody has read.
        frame: SIDEBAR_MENU,
        steps: &[Step::Select(FOCUS), Step::RightClick("workspace-ws-4")],
    },
    Screen {
        name: "other-workspace-menu",
        spec: "sidebar.md",
        world: Some(unread_titled),
        // The first workspace, which isn't showing, and has a reply nobody has read.
        frame: SIDEBAR_MENU,
        steps: &[Step::Select(FOCUS), Step::RightClick("workspace-ws-1")],
    },
    Screen {
        name: "chat-menu",
        spec: "sidebar.md",
        world: Some(unread),
        // The tab of the screen's other chat, the seventh made, which has a reply nobody has read.
        frame: TABS_MENU,
        steps: &[Step::Select(FOCUS), Step::RightClick("chat-chat-7")],
    },
    Screen {
        name: "archive-button",
        spec: "sidebar.md",
        world: Some(conversation),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::Point("workspace-ws-1")],
    },
    Screen {
        name: "archiving",
        spec: "sidebar.md",
        world: Some(conversation),
        // The screen's hub never answers, so the first workspace stays archiving after the pointer leaves it.
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::Point("workspace-ws-1"), Step::Click("archive-ws-1"), Step::MoveAway],
    },
    Screen {
        name: "rename",
        spec: "workspaces.md",
        world: Some(titled),
        // The screen's own workspace is the fourth.
        frame: Frame::Dialog,
        steps: &[Step::Select(FOCUS), Step::RightClick("workspace-ws-4"), Step::ClickItem("Rename…")],
    },
    Screen {
        name: "regenerating-title",
        spec: "workspaces.md",
        world: Some(titled),
        frame: Frame::Exactly(Part::Region(Region::Header), &[]),
        steps: &[Step::Select(FOCUS), Step::Click("regenerate-title"), Step::MoveAway],
    },
    Screen {
        name: "new-workspace",
        spec: "workspaces.md",
        world: Some(titled),
        frame: Frame::Dialog,
        steps: &[Step::Select(FOCUS), Step::Click("new-in-repo-1")],
    },
    Screen {
        name: "new-workspace-written",
        spec: "workspaces.md",
        world: Some(titled),
        frame: Frame::Dialog,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("new-in-repo-1"),
            Step::Click("new-workspace-model"),
            Step::ClickItem("Opus"),
            Step::Click("new-workspace-model"),
            Step::ClickItem("Extra high"),
            Step::PasteImage(badge_to_paste),
            Step::Type(NEW_WORKSPACE_PROMPT),
        ],
    },
    Screen {
        name: "new-workspace-planning",
        spec: "workspaces.md",
        world: Some(titled),
        frame: Frame::Dialog,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("new-in-repo-1"),
            Step::Click("new-workspace-add"),
            Step::ClickItem("Plan mode"),
            Step::MoveAway,
            Step::Type("Plan how the sidebar shows a workspace's pull request"),
        ],
    },
    Screen {
        name: "settings",
        spec: "settings.md",
        world: Some(conversation),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings],
    },
];

/// A screenshot of a bug, pasted into the new-workspace dialog.
fn badge_to_paste() -> Vec<u8> {
    pictures::badge(450, true)
}
