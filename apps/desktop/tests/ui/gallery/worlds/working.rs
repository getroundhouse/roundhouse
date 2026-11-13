//! The screen's chat at work: writing a reply, thinking, working in the background, steered, and with prompts queued.

use protocol::attachment::{Attachment, MediaType};
use protocol::model::{Checks, PullRequest, PullRequestFound, QueuedPrompt, SessionStatus, Wait, WaitUntil};
use protocol::prompt::Prompt;

use super::{chat_world, sidebar};
use crate::gallery::world::World;
use crate::gallery::{chats, pull_requests};

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

/// A turn that's over, with the agent waiting on its pull request's checks in the background, two of them still
/// running. The pull request is on the workspace's record, so the app knows it before it makes the chat, which starts
/// out with where it stands.
pub(in crate::gallery) fn waiting_on_checks() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let chat = world.chat(&focus, SessionStatus::Background, chats::waiting());
    let wait = Wait {
        id: "w1".into(),
        session_id: chat.clone(),
        workspace_id: focus.clone(),
        number: 182,
        call: Some("toolu-1".into()),
        until: WaitUntil::Checks,
        watch: None,
        started_at_ms: 0,
        ended: None,
    };
    world
        .sessions
        .iter_mut()
        .filter(|session| session.id == chat)
        .for_each(|session| session.wait = Some(wait.clone()));
    let checks = Checks { passed: 7, failed: 0, pending: 2 };
    let opened =
        PullRequest { number: 182, title: "Show PR state in the sidebar".to_string(), checks, ..pull_requests::open() };
    world.workspace_mut(&focus).pull_request = Some(PullRequestFound { pull_request: Some(opened) });
    world
}

/// Two independent remote waits, one of them spanning a stack.
pub(in crate::gallery) fn waiting_on_stacks() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let chat = world.chat(
        &focus,
        SessionStatus::Background,
        chats::answered(
            "Watch #7's stack for required checks and also wait for #9 to merge.",
            "I'm watching #6 and #7 for required checks, and #9 for its merge.",
        ),
    );
    let mut wait = protocol::fixtures::pull_request_wait("stack");
    wait.session_id = chat.clone();
    wait.workspace_id = focus.clone();
    let watch = wait.watch.as_mut().unwrap();
    watch.snapshot.pull_requests[0].dependencies = vec![protocol::model::PullRequestNumber::try_from(6).unwrap()];
    watch.snapshot.pull_requests.insert(0, protocol::fixtures::observed_pull_request(6));
    let mut second = Wait { id: "merge".into(), until: WaitUntil::Merged, ..wait.clone() };
    second.number = 9;
    let watch = second.watch.as_mut().unwrap();
    watch.requested =
        protocol::model::PullRequestSelection::try_from(vec![protocol::model::PullRequestNumber::try_from(9).unwrap()])
            .unwrap();
    watch.snapshot.pull_requests = vec![protocol::fixtures::observed_pull_request(9)];
    world.sessions.iter_mut().filter(|session| session.id == chat).for_each(|session| {
        session.waits = Some(vec![wait.clone(), second.clone()]);
    });
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

/// Partway through a thought, with a prompt sent meanwhile that it hasn't taken yet, under the thought.
pub(in crate::gallery) fn steered_thinking() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let chat = world.chat(&focus, SessionStatus::Running, chats::steered_thinking());
    world.tails.insert(chat.clone(), chats::thinking_tail());
    world.working_for(&chat, 14_200);
    world
}

fn three_queued() -> Vec<QueuedPrompt> {
    vec![
        QueuedPrompt {
            client_msg_id: "queued-1".into(),
            prompt: Prompt::carrying(
                "Once that's in, add the PR state to the header as well, as in this sketch.",
                vec![Attachment {
                    id: "sketch".into(),
                    name: "header sketch.png".to_string(),
                    media_type: MediaType::Png,
                    size_bytes: 48_213,
                }],
            ),
        },
        QueuedPrompt { client_msg_id: "queued-2".into(), prompt: Prompt::new("Then open a draft PR for all of it.") },
        QueuedPrompt {
            client_msg_id: "queued-3".into(),
            prompt: Prompt::new(
                "After that, look at whether the CLI should show it too. It lists workspaces with their \
                   branches, so the state could go beside each one, but only if it fits on one line at 80 columns."
                    .to_string(),
            ),
        },
    ]
}

/// Writing a reply, with three prompts queued for after it.
pub(in crate::gallery) fn queued() -> World {
    let mut world = streaming();
    world.sessions.last_mut().unwrap().queued = three_queued();
    world
}

/// Stopped, with three prompts the queue holds since, the last of them carrying two files.
pub(in crate::gallery) fn queue_held() -> World {
    let mut world = chat_world(SessionStatus::Idle, chats::stopped());
    let chat = world.sessions.last_mut().unwrap();
    chat.queued = three_queued();
    let file =
        |name: &str, media_type| Attachment { id: name.into(), name: name.to_string(), media_type, size_bytes: 2_048 };
    chat.queued[2].prompt.files = vec![file("workspaces.txt", MediaType::Text), file("header.png", MediaType::Png)];
    chat.queue_held = true;
    world
}
