//! The screen's chat at work: writing a reply, thinking, working in the background, steered, and with prompts queued.

use protocol::attachment::{Attachment, MediaType};
use protocol::model::{
    CheckState, PullRequestNumber, PullRequestSelection, QueueState, QueuedPrompt, SessionStatus, Wait, WaitUntil,
};
use protocol::prompt::Prompt;

use super::{chat_world, sidebar};
use crate::gallery::pull_requests;
use client_core::fake_hub::World;
use protocol::prompt::PromptFrom;
use spec_screens::chats;

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

/// A turn that's over, with the agent waiting on its pull request's required checks in the background, two of them
/// still running, as the hub last read them.
pub(in crate::gallery) fn waiting_on_checks() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let chat = world.chat(&focus, SessionStatus::Background, chats::waiting());
    let mut opened = pull_requests::checked(pull_requests::open(), 7, 0, 2);
    opened.pull_request.number = PullRequestNumber::try_from(182).unwrap();
    opened.pull_request.summary.said.about.title = "Show PR state in the sidebar".to_string();
    let mut wait = protocol::fixtures::pull_request_wait("w1");
    wait.session_id = chat.clone();
    wait.workspace_id = focus.clone();
    wait.call = Some("toolu-1".into());
    wait.number = 182;
    let watch = wait.watch.as_mut().unwrap();
    watch.requested = PullRequestSelection::try_from(vec![PullRequestNumber::try_from(182).unwrap()]).unwrap();
    watch.snapshot.pull_requests = vec![opened.pull_request.clone()];
    world.sessions.iter_mut().filter(|session| session.id == chat).for_each(|session| {
        session.waits = Some(vec![wait.clone()]);
    });
    world.github_says.pull_requests.insert(focus, opened);
    world
}

/// Two independent remote waits: one on a stack's required checks, and one until a merge, on a pull request whose
/// checks are running in the merge queue.
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
    let stack = &mut watch.snapshot.pull_requests[0].stack.as_mut().unwrap().said;
    stack.dependencies = vec![PullRequestNumber::try_from(6).unwrap()];
    watch.snapshot.pull_requests.insert(0, protocol::fixtures::pull_request(6));
    let mut second = Wait { id: "merge".into(), until: WaitUntil::Merged, ..wait.clone() };
    second.number = 9;
    let watch = second.watch.as_mut().unwrap();
    watch.requested =
        protocol::model::PullRequestSelection::try_from(vec![protocol::model::PullRequestNumber::try_from(9).unwrap()])
            .unwrap();
    let mut queued = protocol::fixtures::pull_request(9);
    queued.checks.as_mut().unwrap().said.checks[0].state = CheckState::Passed;
    queued.summary.said.queue = Some(QueueState::AwaitingChecks);
    watch.snapshot.pull_requests = vec![queued];
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
            from: PromptFrom::Person,
        },
        QueuedPrompt {
            client_msg_id: "queued-2".into(),
            prompt: Prompt::new("Then open a draft PR for all of it."),
            from: PromptFrom::Person,
        },
        QueuedPrompt {
            client_msg_id: "queued-3".into(),
            prompt: Prompt::new(
                "After that, look at whether the CLI should show it too. It lists workspaces with their \
                   branches, so the state could go beside each one, but only if it fits on one line at 80 columns."
                    .to_string(),
            ),
            from: PromptFrom::Person,
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
