//! What the hub holds for each screen in the gallery, built on [`World`].

#![cfg(test)]

use protocol::attachment::{Attachment, MediaType};
use protocol::model::{
    AttachmentId, CommentId, DiffComment, DiffSide, GitDiff, PullRequest, QueuedPrompt, Rev, SessionStatus,
    SetupStatus, Title, WorkspaceId, WorkspaceStatus,
};

use super::screens::FOCUS;
use super::world::{HubBuild, World};
use super::{chats, patch, pictures};

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

/// Two repositories, with a workspace in each state a sidebar row can show, and an archived one that stays hidden.
/// Returns the screen's own workspace, which comes after them.
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
    focus
}

pub(super) fn conversation() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    converse(&mut world, &focus);
    world
}

/// The finished turn in the screen's own workspace, on a branch with a pull request open.
fn converse(world: &mut World, focus: &WorkspaceId) {
    world.workspace_mut(focus).branch = "sidebar-pr-state".to_string();
    world.chat(focus, SessionStatus::Idle, chats::finished());
    world.pull_requests.insert(
        focus.clone(),
        PullRequest {
            number: 182,
            url: "https://example.com/pull/182".to_string(),
            title: "Show PR state in the sidebar".to_string(),
            state: "OPEN".to_string(),
            is_draft: false,
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
    let mut world = conversation();
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
    world.tails.insert(chat, chats::streaming_tail());
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
    let shot = pictures::overlapping_badge();
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

pub(super) fn diff() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    world.chat(&focus, SessionStatus::Idle, chats::finished());
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
