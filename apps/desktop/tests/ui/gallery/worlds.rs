//! What the hub holds for each screen in the gallery, built on [`World`].

#![cfg(test)]

use protocol::attachment::{Attachment, MediaType};
use protocol::model::{
    AgentAccount, AttachmentId, Checks, CommentId, ContextPart, ContextPartKind, ContextUsage, DiffComment, DiffSide,
    GitDiff, LimitWindow, MergeState, PullRequest, QueuedPrompt, Rev, SessionAccount, SessionStatus, SetupStatus,
    Title, UsageLimit, WorkspaceId, WorkspaceStatus,
};

use super::screens::FOCUS;
use super::world::{HubBuild, NOW_MS, World};
use super::{chats, file_chats, image_chats, patch, pictures, pull_requests, worktree};

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
fn sidebar(world: &mut World) -> WorkspaceId {
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

/// The conversation, in a chat whose agent has said how full its context is, on an account with a week's limit that
/// starts again in six days.
pub(super) fn context() -> World {
    const HOUR_MS: i64 = 3_600_000;
    let mut world = conversation();
    let part = |name: &str, tokens, kind| ContextPart { name: name.to_string(), tokens, kind };
    world.sessions.last_mut().unwrap().context = Some(ContextUsage {
        used_tokens: 313_500,
        max_tokens: 500_000,
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
        heard_at_ms: Some(NOW_MS - HOUR_MS),
    };
    let account = AgentAccount {
        id: Some("claude-0000000000000000".into()),
        logged_in: true,
        model_provider: Some("Anthropic API".to_string()),
        plan: Some("Max".to_string()),
        login: Some("Claude account".to_string()),
        email: Some("ada@example.com".to_string()),
    };
    world.account = Some(SessionAccount { account, limits: vec![week] });
    world
}

/// A chat whose one turn is over. It took a little longer as the hub saw it than the agent counted, and the hub's count
/// is the one shown.
fn finished_chat(world: &mut World, workspace: &WorkspaceId) {
    let chat = world.chat(workspace, SessionStatus::Idle, chats::finished());
    world.last_turn_took(&chat, 108_000);
}

/// The finished turn in the screen's own workspace, on a branch with a pull request open.
fn converse(world: &mut World, focus: &WorkspaceId) {
    world.workspace_mut(focus).branch = "sidebar-pr-state".to_string();
    finished_chat(world, focus);
    world.pull_requests.insert(
        focus.clone(),
        PullRequest {
            number: 182,
            url: "https://example.com/pull/182".to_string(),
            title: "Show PR state in the sidebar".to_string(),
            base: Some("main".to_string()),
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

pub(super) fn streaming() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let chat = world.chat(&focus, SessionStatus::Running, chats::streaming());
    world.tails.insert(chat.clone(), chats::streaming_tail());
    world.working_for(&chat, 72_400);
    world
}

/// Still working, with a prompt sent meanwhile that it hasn't taken yet.
pub(super) fn steered() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let chat = world.chat(&focus, SessionStatus::Running, chats::steered());
    world.working_for(&chat, 31_600);
    world
}

/// Still working on a turn, with what it started going in the background, and writing its next call.
pub(super) fn working_background() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let chat = world.chat(&focus, SessionStatus::Running, chats::started_in_background());
    world.tails.insert(chat.clone(), chats::calling_tail());
    world.working_for(&chat, 48_300);
    world
}

/// Partway through a thought.
pub(super) fn thinking_streaming() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let chat = world.chat(&focus, SessionStatus::Running, chats::thinking_so_far());
    world.tails.insert(chat.clone(), chats::thinking_tail());
    world.working_for(&chat, 12_700);
    world
}

fn three_queued() -> Vec<QueuedPrompt> {
    vec![
        QueuedPrompt {
            client_msg_id: "queued-1".into(),
            text: "Once that's in, add the PR state to the header as well.".to_string(),
        },
        QueuedPrompt { client_msg_id: "queued-2".into(), text: "Then open a draft PR for all of it.".to_string() },
        QueuedPrompt {
            client_msg_id: "queued-3".into(),
            text: "After that, look at whether the CLI should show it too. It lists workspaces with their \
                   branches, so the state could go beside each one, but only if it fits on one line at 80 columns."
                .to_string(),
        },
    ]
}

/// Writing a reply, with three prompts queued for after it.
pub(super) fn queued() -> World {
    let mut world = streaming();
    world.sessions.last_mut().unwrap().queued = three_queued();
    world
}

/// Stopped, with three prompts the queue holds since.
pub(super) fn queue_held() -> World {
    let mut world = chat_world(SessionStatus::Idle, chats::stopped());
    let chat = world.sessions.last_mut().unwrap();
    chat.queued = three_queued();
    chat.queue_held = true;
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
    let shot = pictures::overlapping_badge(450);
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
    let shot = pictures::overlapping_badge(300);
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
        GitDiff { base_commit: Some("3f4c19c".to_string()), patch: patch::PATCH.to_string(), truncated: false },
    );
    world.comments.insert(
        focus.clone(),
        vec![DiffComment {
            id: CommentId::from("comment-1"),
            workspace_id: focus,
            path: "crates/protocol/src/model.rs".to_string(),
            side: DiffSide::New,
            line: 101,
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
