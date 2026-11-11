//! The screen's chat at work: writing a reply, thinking, working in the background, steered, and with prompts queued.

use protocol::model::{QueuedPrompt, SessionStatus};

use super::{chat_world, sidebar};
use crate::gallery::chats;
use crate::gallery::world::World;

pub(in crate::gallery) fn streaming() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let chat = world.chat(&focus, SessionStatus::Running, chats::streaming());
    world.tails.insert(chat.clone(), chats::streaming_tail());
    world.working_for(&chat, 72_400);
    world
}

/// Still working, with a prompt sent meanwhile that it hasn't taken yet.
pub(in crate::gallery) fn steered() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let chat = world.chat(&focus, SessionStatus::Running, chats::steered());
    world.working_for(&chat, 31_600);
    world
}

/// Still working on a turn, with what it started going in the background, and writing its next call.
pub(in crate::gallery) fn working_background() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let chat = world.chat(&focus, SessionStatus::Running, chats::started_in_background());
    world.tails.insert(chat.clone(), chats::calling_tail());
    world.working_for(&chat, 48_300);
    world
}

/// Partway through a thought.
pub(in crate::gallery) fn thinking_streaming() -> World {
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
pub(in crate::gallery) fn queued() -> World {
    let mut world = streaming();
    world.sessions.last_mut().unwrap().queued = three_queued();
    world
}

/// Stopped, with three prompts the queue holds since.
pub(in crate::gallery) fn queue_held() -> World {
    let mut world = chat_world(SessionStatus::Idle, chats::stopped());
    let chat = world.sessions.last_mut().unwrap();
    chat.queued = three_queued();
    chat.queue_held = true;
    world
}
