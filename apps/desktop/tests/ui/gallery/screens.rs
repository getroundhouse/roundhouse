//! The screens the gallery draws: what the hub holds for each, from `worlds.rs`, and what to do in the app before
//! taking it, in the steps `steps.rs` has.

#![cfg(test)]

use protocol::model::SessionStatus;

use super::steps::{Screen, Step, Target};
use super::world::World;
use super::worlds::{
    approval, attachments, chat_tabs, chat_world, closed_chats, context, context_without_limits, conversation, diff,
    finishing_update, own_hub, planning, queue_held, queued, setup_failed, steered, streaming, titled, unread,
    unread_titled, working_background,
};
use super::{chats, long_chats, pictures, worktree};

/// The workspace each screen is about, in the first repository, below the ones the sidebar always has.
pub(super) const FOCUS: &str = "aspen";

/// A task described in the new-workspace dialog.
const NEW_WORKSPACE_PROMPT: &str = "When the sidebar is narrow, the PR badge runs over the workspace's name. Let the name \
                                    shrink and end in an ellipsis, and keep the badge at its own width.";

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
    Screen {
        name: "turn-card",
        spec: "chats.md",
        world: Some(conversation),
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Id(chats::FINISHED_TURN_END), shows: "turn-card" }],
    },
    Screen {
        name: "turn-card-midnight",
        spec: "chats.md",
        world: Some(|| conversation().last_turn_past_midnight()),
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Id(chats::FINISHED_TURN_END), shows: "turn-card" }],
    },
    Screen { name: "streaming", spec: "chats.md", world: Some(streaming), steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "context",
        spec: "chats.md",
        world: Some(context),
        // The bars come once the hub has said what's left of the limits, which the picture is taken with.
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("context"), shows: "context-card" },
        ],
    },
    Screen {
        name: "context-account",
        spec: "chats.md",
        world: Some(context_without_limits),
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Id("context"), shows: "account" }],
    },
    Screen {
        name: "usage",
        spec: "chats.md",
        world: Some(context),
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("usage"), shows: "usage-card" },
        ],
    },
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
    Screen { name: "approval", spec: "chats.md", world: Some(approval), steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "background",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Background, chats::background())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "working-background",
        spec: "chats.md",
        world: Some(working_background),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "question",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, chats::question())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "questions",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, chats::questions())),
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
    Screen { name: "steered", spec: "chats.md", world: Some(steered), steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "queued",
        spec: "chats.md",
        world: Some(queued),
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Id("queued-queued-2"), shows: "edit-queued-queued-2" }],
    },
    Screen { name: "queue-held", spec: "chats.md", world: Some(queue_held), steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "outline",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, long_chats::many_turns())),
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id("outline"), shows: "outline-list" },
            Step::Click(long_chats::FOURTEENTH_PROMPT),
            Step::MoveAway,
            Step::LetTheScrollbarFade,
        ],
    },
    Screen {
        name: "outline-list",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, long_chats::many_turns())),
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id("outline"), shows: "outline-list" },
            Step::Click(long_chats::FOURTEENTH_PROMPT),
            Step::MoveAway,
            Step::Hover { on: Target::Id("outline"), shows: "outline-list" },
            Step::LetTheScrollbarFade,
        ],
    },
    Screen {
        name: "plan-mode",
        spec: "chats.md",
        world: Some(planning),
        steps: &[Step::Select(FOCUS), Step::Type("Plan how a workspace's row shows its pull request")],
    },
    Screen {
        name: "model-picker",
        spec: "chats.md",
        world: Some(conversation),
        steps: &[Step::Select(FOCUS), Step::Click("model-picker")],
    },
    Screen {
        name: "composer-menu",
        spec: "chats.md",
        world: Some(conversation),
        steps: &[Step::Select(FOCUS), Step::Click("add")],
    },
    Screen {
        name: "composer-menu-planning",
        spec: "chats.md",
        world: Some(planning),
        steps: &[Step::Select(FOCUS), Step::Click("add")],
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
    Screen {
        name: "all-files",
        spec: "files.md",
        world: Some(diff),
        steps: &[
            Step::Select(FOCUS),
            Step::AllFiles,
            Step::Click("all-folder-.context"),
            Step::WaitFor("all-folder-.context/gallery"),
            Step::Click("all-folder-crates"),
            Step::Click("all-folder-crates/protocol"),
            Step::Click("all-folder-crates/protocol/src"),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "file-opened",
        spec: "files.md",
        world: Some(diff),
        steps: &[
            Step::Select(FOCUS),
            Step::AllFiles,
            Step::Click("all-folder-.context"),
            Step::WaitFor("all-folder-.context/gallery"),
            Step::Click("all-folder-crates"),
            Step::Click("all-folder-crates/protocol"),
            Step::Click("all-folder-crates/protocol/src"),
            Step::ViewFile(worktree::OPENED),
            Step::MoveAway,
        ],
    },
    Screen { name: "unread", spec: "sidebar.md", world: Some(unread), steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "sidebar-hidden",
        spec: "sidebar.md",
        world: Some(conversation),
        // The pointer is left on the workspace's name, which has nothing to show for it, rather than on the button.
        steps: &[Step::Select(FOCUS), Step::Click("toggle-sidebar"), Step::Point("workspace-title")],
    },
    Screen { name: "home", spec: "home.md", world: Some(titled), steps: &[Step::Select(FOCUS), Step::Click("home")] },
    Screen {
        name: "workspace-menu",
        spec: "sidebar.md",
        world: Some(unread_titled),
        // The screen's own workspace, the fourth, whose other chat has a reply nobody has read.
        steps: &[Step::Select(FOCUS), Step::RightClick("workspace-ws-4")],
    },
    Screen {
        name: "other-workspace-menu",
        spec: "sidebar.md",
        world: Some(unread_titled),
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
        name: "archiving",
        spec: "sidebar.md",
        world: Some(conversation),
        // The screen's hub never answers, so the first workspace stays archiving after the pointer leaves it.
        steps: &[Step::Select(FOCUS), Step::Point("workspace-ws-1"), Step::Click("archive-ws-1"), Step::MoveAway],
    },
    Screen {
        name: "rename",
        spec: "workspaces.md",
        world: Some(titled),
        // The screen's own workspace is the fourth.
        steps: &[Step::Select(FOCUS), Step::RightClick("workspace-ws-4"), Step::ClickItem("Rename…")],
    },
    Screen {
        name: "regenerating-title",
        spec: "workspaces.md",
        world: Some(titled),
        steps: &[Step::Select(FOCUS), Step::Click("regenerate-title"), Step::MoveAway],
    },
    Screen {
        name: "new-workspace",
        spec: "workspaces.md",
        world: Some(titled),
        steps: &[Step::Select(FOCUS), Step::Click("new-in-repo-1")],
    },
    Screen {
        name: "new-workspace-written",
        spec: "workspaces.md",
        world: Some(titled),
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

/// A screenshot of a bug, pasted into the new-workspace dialog.
fn badge_to_paste() -> Vec<u8> {
    pictures::overlapping_badge(450)
}
