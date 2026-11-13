//! What the hub holds for each screen in the gallery, built on [`World`].

#![cfg(test)]

use protocol::attachment::{Attachment, MediaType};
use protocol::model::{
    Account, AttachmentId, Checks, CommentId, CommitId, ContextPart, ContextPartKind, ContextUsage, DiffComment,
    DiffSide, FileContents, GitDiff, LimitWindow, MaxSetBy, MergeState, PermissionMode, PullRequest, Rev,
    SessionStatus, SetupStatus, Title, UsageLimit, WorkspaceId, WorkspaceStatus,
};

use super::screens::FOCUS;
use super::world::{HubBuild, NOW_MS, World};
use super::{chats, file_chats, image_chats, patch, pictures, pull_requests, worktree};

mod working;

pub(super) use working::{
    queue_held, queued, steered, steered_thinking, streaming, thinking_streaming, waiting_on_checks, working_background,
};

/// The conversation, on an older hub that a client started, with chats still running on it.
pub(super) fn finishing_update() -> World {
    older_hub(true)
}

/// The conversation, on an older hub that someone runs themselves.
pub(super) fn own_hub() -> World {
    older_hub(false)
}

fn older_hub(on_demand: bool) -> World {
    let mut world = conversation();
    world.hub = Some(HubBuild { version: "0.9.0", number: 1, on_demand });
    world
}

/// Two repositories, with a workspace in each state a sidebar row can show, and an archived one that stays hidden,
/// and the files in a worktree, which All files lists in a workspace that changes nothing. Returns the screen's own
/// workspace, which comes after the first repository's others.
pub(super) fn sidebar(world: &mut World) -> WorkspaceId {
    let roundhouse = world.repo("roundhouse");
    let signal_box = world.repo("signal-box");
    for (name, status) in
        [("alder", SessionStatus::Idle), ("birch", SessionStatus::Running), ("cedar", SessionStatus::WaitingApproval)]
    {
        let id = world.workspace(&roundhouse, name);
        world.chat(&id, status, Vec::new());
    }
    let focus = world.workspace(&roundhouse, FOCUS);
    for (name, status) in [("elm", SessionStatus::Interrupted), ("fir", SessionStatus::Idle)] {
        let id = world.workspace(&signal_box, name);
        world.chat(&id, status, Vec::new());
    }
    let archived = world.workspace(&signal_box, "hazel");
    world.workspace_mut(&archived).status = WorkspaceStatus::Archived;
    worktree::put_in(world);
    focus
}

pub(super) fn conversation() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    converse(&mut world, &focus);
    world
}

/// The conversation, with the first workspace being archived, as its record says, whichever app asked.
pub(super) fn archiving() -> World {
    let mut world = conversation();
    world.workspace_mut(&WorkspaceId::from("ws-1")).status = WorkspaceStatus::Archiving;
    world
}

/// The conversation, in a chat whose agent has said how full its context is, on an account with a five-hour limit
/// that starts again in three hours and a week's limit that starts again in six days.
pub(super) fn context() -> World {
    const HOUR_MS: i64 = 3_600_000;
    let mut world = conversation();
    let part = |name: &str, tokens, kind| ContextPart { name: name.to_string(), tokens, kind };
    world.sessions.last_mut().unwrap().context = Some(ContextUsage {
        used_tokens: 313_500,
        max_tokens: 500_000,
        model_tokens: None,
        max_set_by: None,
        parts: vec![
            part("System prompt", 4_000, ContextPartKind::Used),
            part("System tools", 4_500, ContextPartKind::Used),
            part("System tools (deferred)", 19_500, ContextPartKind::Deferred),
            part("MCP tools", 500, ContextPartKind::Used),
            part("MCP tools (deferred)", 11_500, ContextPartKind::Deferred),
            part("Skills", 6_000, ContextPartKind::Used),
            part("Messages", 298_500, ContextPartKind::Used),
            part("Autocompact buffer", 33_000, ContextPartKind::Buffer),
            part("Free space", 153_500, ContextPartKind::Free),
        ],
    });
    let week = UsageLimit {
        window: LimitWindow::SevenDays,
        used_percent: 4,
        resets_at_ms: Some(NOW_MS + 150 * HOUR_MS),
        // As an account's record has it, which doesn't say when.
        heard_at_ms: None,
    };
    let hours = UsageLimit {
        window: LimitWindow::FiveHours,
        used_percent: 29,
        resets_at_ms: Some(NOW_MS + 3 * HOUR_MS),
        heard_at_ms: None,
    };
    let account = Account {
        id: "claude-0000000000000000".into(),
        model_provider: Some("Anthropic API".to_string()),
        plan: Some("Max".to_string()),
        login: Some("Claude account".to_string()),
        email: Some("ada@example.com".to_string()),
        name: None,
        limits: vec![hours, week],
        held_login: None,
        rev: Rev::default(),
    };
    world.sessions.last_mut().unwrap().account = Some(account.id.clone());
    world.accounts.push(account);
    world
}

/// The same, nearly full, where the agent's settings make the context half of what its model holds.
pub(super) fn context_capped() -> World {
    let mut world = context();
    let context = world.sessions.last_mut().unwrap().context.as_mut().unwrap();
    (context.used_tokens, context.model_tokens, context.max_set_by) =
        (465_200, Some(1_000_000), Some(MaxSetBy::Settings));
    for part in &mut context.parts {
        part.tokens = match part.name.as_str() {
            "Messages" => 450_200,
            "Free space" => 1_800,
            _ => part.tokens,
        };
    }
    world
}

/// The same, before any agent under the account has heard of a limit.
pub(super) fn context_without_limits() -> World {
    let mut world = context();
    world.accounts[0].limits.clear();
    world
}

/// A chat whose one turn is over. It took a little longer as the hub saw it than the agent counted, and the hub's count
/// is the one shown.
pub(super) fn finished_chat(world: &mut World, workspace: &WorkspaceId) {
    let chat = world.chat(workspace, SessionStatus::Idle, chats::finished());
    world.last_turn_took(&chat, 108_000);
}

/// The conversation, with the screen's chat in plan mode.
pub(super) fn planning() -> World {
    in_plan_mode(conversation())
}

/// The screen's chat in plan mode, waiting for a command to be allowed, since only an agent that's planning asks
/// before using a tool.
pub(super) fn approval() -> World {
    in_plan_mode(chat_world(SessionStatus::WaitingApproval, chats::approval()))
}

/// `world`, with the screen's chat in plan mode.
pub(super) fn in_plan_mode(mut world: World) -> World {
    let focus = world.workspaces.iter().find(|w| w.name == FOCUS).unwrap().id.clone();
    let chat = world.sessions.iter_mut().find(|s| s.workspace_id == focus).unwrap();
    chat.permission_mode = PermissionMode::Plan;
    world
}

/// The finished turn in the screen's own workspace, on a branch with a pull request open.
fn converse(world: &mut World, focus: &WorkspaceId) {
    world.workspace_mut(focus).branch = "sidebar-pr-state".into();
    finished_chat(world, focus);
    world.pull_requests.insert(
        focus.clone(),
        PullRequest {
            number: 182,
            url: "https://example.com/pull/182".to_string(),
            title: "Show PR state in the sidebar".to_string(),
            base: Some("main".into()),
            merge_state: MergeState::Clean,
            checks: Checks { passed: 5, failed: 0, pending: 0 },
            ..pull_requests::open()
        },
    );
}

/// Replies nobody has read: in a workspace that isn't showing, and in the chat that isn't showing in this one. The chat
/// showing is read, as it is on screen.
pub(super) fn unread() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let alder = world.workspaces.iter().find(|w| w.name == "alder").unwrap().id.clone();
    let reply = chats::answered(
        "Why does the hub log a warning on every reconnect?",
        "It logs one each time a client goes without saying goodbye. That's ordinary, so it should be a debug line.",
    );
    let elsewhere = world.chat(&alder, SessionStatus::Idle, reply);
    world.unread(&elsewhere);
    // Older than the conversation, so the app opens the workspace on that instead, as it opens the chat last worked in.
    let reply = chats::answered(
        "Which colours does GitHub use for a PR's state?",
        "Green for open, purple for merged, red for closed, and grey for a draft.",
    );
    let other_chat = world.chat(&focus, SessionStatus::Idle, reply);
    world.unread(&other_chat);
    converse(&mut world, &focus);
    world
}

/// Three chats in the screen's own workspace, the conversation showing, all of them read.
pub(super) fn chat_tabs() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    // Older than the conversation, so the app opens the workspace on that, as it opens the chat last worked in.
    for (prompt, reply) in [
        (
            "Which colours does GitHub use for a PR's state?",
            "Green for open, purple for merged, red for closed, and grey for a draft.",
        ),
        (
            "Why does the hub log a warning on every reconnect?",
            "It logs one each time a client goes without saying goodbye.",
        ),
    ] {
        world.chat(&focus, SessionStatus::Idle, chats::answered(prompt, reply));
    }
    converse(&mut world, &focus);
    world
}

/// The conversation showing, and two chats in its workspace that were closed, one a few minutes ago and one hours ago.
pub(super) fn closed_chats() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    for (prompt, reply, minutes_before_now) in [
        (
            "Why does the hub log a warning on every reconnect?",
            "It logs one each time a client goes without saying goodbye.",
            5 * 60,
        ),
        (
            "Which colours does GitHub use for a PR's state?",
            "Green for open, purple for merged, red for closed, and grey for a draft.",
            9,
        ),
    ] {
        let chat = world.chat(&focus, SessionStatus::Idle, chats::answered(prompt, reply));
        world.close(&chat, minutes_before_now);
    }
    converse(&mut world, &focus);
    world
}

/// The conversation, with every workspace titled from its first prompt.
pub(super) fn titled() -> World {
    with_titles(conversation())
}

/// [`unread`], with every workspace titled, so a row's menu offers a new title.
pub(super) fn unread_titled() -> World {
    with_titles(unread())
}

fn with_titles(mut world: World) -> World {
    let titles = [
        ("alder", "Fix the login redirect"),
        ("birch", "Speed up opening a large diff"),
        ("cedar", "Ask before deleting files"),
        (FOCUS, "Show PR state in the sidebar"),
        ("elm", "Retry a failed setup"),
        ("fir", "Read settings from TOML"),
    ];
    for workspace in &mut world.workspaces {
        workspace.title =
            titles.iter().find(|(name, _)| *name == workspace.name).and_then(|(_, title)| Title::tidied(title));
    }
    world
}

/// A chat in the screen's own workspace, and nothing else to see.
pub(super) fn chat_world(status: SessionStatus, events: Vec<protocol::event::EventBody>) -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    world.chat(&focus, status, events);
    world
}

/// The screen's chat, with a prompt that carried a screenshot and a log.
pub(super) fn attachments() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let shot = pictures::badge(450, true);
    let attached = [
        ("shot-1", "Screenshot 2026-11-01 at 10.24.png", MediaType::Png, shot),
        ("log-1", "layout.log", MediaType::Text, chats::LAYOUT_LOG.as_bytes().to_vec()),
    ];
    let mut carried = Vec::new();
    for (id, name, media_type, data) in attached {
        let size_bytes = u64::try_from(data.len()).unwrap();
        carried.push(Attachment { id: AttachmentId::from(id), name: name.to_string(), media_type, size_bytes });
        world.attachments.insert(AttachmentId::from(id), data);
    }
    world.chat(&focus, SessionStatus::Idle, chats::attached(carried));
    world
}

/// The screen's chat, where the reply points to a screenshot in the worktree, which the hub kept.
pub(super) fn pointed() -> World {
    let board = pictures::signal_board();
    let size_bytes = u64::try_from(board.len()).unwrap();
    let screenshot = Attachment {
        id: AttachmentId::from("after-1"),
        name: "board-after.png".into(),
        media_type: MediaType::Png,
        size_bytes,
    };
    let mut world = chat_world(SessionStatus::Idle, image_chats::pointed(screenshot.clone()));
    world.attachments.insert(screenshot.id, board);
    world
}

/// The screen's chat, with the agent reading a screenshot, which the hub keeps.
pub(super) fn read_image() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    // Short enough that the call it opens from stays in sight above it, under the composer.
    let shot = pictures::badge(300, true);
    let size_bytes = u64::try_from(shot.len()).unwrap();
    let image = Attachment {
        id: AttachmentId::from("read-1"),
        name: "image".to_string(),
        media_type: MediaType::Png,
        size_bytes,
    };
    world.attachments.insert(image.id.clone(), shot);
    world.chat(&focus, SessionStatus::Idle, file_chats::read_image(image));
    world
}

pub(super) fn diff() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    finished_chat(&mut world, &focus);
    world.diffs.insert(
        focus.clone(),
        GitDiff { base_commit: Some(CommitId::from(patch::BASE_COMMIT)), patch: patch::patch(), truncated: false },
    );
    // So the lines git left out of the file the screens open can be shown.
    world.base_files.insert(worktree::OPENED.to_string(), FileContents::Text { text: worktree::MODEL.to_string() });
    world.comments.insert(
        focus.clone(),
        vec![DiffComment {
            id: CommentId::from("comment-1"),
            workspace_id: focus,
            path: "crates/protocol/src/model.rs".to_string(),
            side: DiffSide::New,
            line: patch::PULL_REQUEST_LINE,
            text: "Should this say when it was last checked, so a stale state can show as stale?".to_string(),
            created_at_ms: 0,
            rev: Rev::default(),
        }],
    );
    world
}

/// A new workspace whose setup script failed, with its log open and nothing said in its chat yet.
pub(super) fn setup_failed() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    world.workspace_mut(&focus).setup = SetupStatus::Failed { code: Some(1) };
    world.chat(&focus, SessionStatus::Idle, Vec::new());
    world.setup_logs.insert(focus, chats::SETUP_LOG.to_string());
    world
}
