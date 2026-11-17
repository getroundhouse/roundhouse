//! What workspaces and chats are called. One example of a title the agent gives, through a client, a hub, a worker and
//! the agent, with a prompt's image, and what a worker does with a call for a title as its workspace is archived: the
//! hub's own tests, in its `hub/titles`, have where each title comes from, and when it stays.

#![cfg(test)]

use std::path::Path;
use std::time::{Duration, Instant};

use client_core::Client;
use protocol::attachment::{Attachment, MediaType};
use protocol::model::{AttachmentId, Session, TitleSource};
use protocol::prompt::Prompt;

use crate::support::{Stack, is_alive, wait_until, workspace};

/// Has the fake agent give `title`, and hold the call for it, then prompts the chat, and waits for the call.
fn prompt_for_a_held_title(stack: &Stack, client: &Client, chat: &Session, title: &str) -> std::path::PathBuf {
    let state = stack.agent_state();
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("title"), title).unwrap();
    std::fs::write(state.join("title-held"), "").unwrap();
    client.session_prompt(&chat.id, "say:fix the login\nit loops", None).unwrap();
    wait_until("the call for a title", || state.join("title-pids").exists());
    state
}

/// Adds a file to the chat for a prompt to carry. An image is enough of a PNG for anything that only looks at how it
/// starts, which is all the hub and the fake agent do.
fn add(client: &Client, chat: &Session, name: &str, media_type: MediaType) -> Attachment {
    let data = if media_type == MediaType::Png {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.resize(300, 7);
        png
    } else {
        b"error at line 42".to_vec()
    };
    let id = AttachmentId::from(name.replace('.', "-"));
    client.attachment_add(&chat.id, &id, name, media_type, data).unwrap()
}

/// What the fake agent was last given with a prompt to title, as it describes each block.
fn given_with(state: &Path) -> String {
    std::fs::read_to_string(state.join("title-images")).unwrap()
}

/// Has the fake agent title prompts, and returns where it keeps what it was given.
fn ready_to_title(stack: &Stack) -> std::path::PathBuf {
    let state = stack.agent_state();
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("title"), "Fix the queue error").unwrap();
    state
}

/// A prompt such as "how do we fix this?" can be about a screenshot, so the model is given its images, and only them.
#[test]
fn a_prompts_images_go_with_it_for_its_title() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, chat) = stack.workspace(&client);
    let state = ready_to_title(&stack);
    let carried =
        vec![add(&client, &chat, "shot.png", MediaType::Png).id, add(&client, &chat, "build.log", MediaType::Text).id];

    client.session_prompt_with(&chat.id, None, Prompt::carrying("say:how do we fix this?", carried)).unwrap();

    wait_until("the chat's generated title", || {
        client.session_get(&chat.id).unwrap().title_source == TitleSource::Generated
    });
    assert_eq!(workspace(&client, &made.id).title.as_deref(), Some("Fix the queue error"));
    assert_eq!(given_with(&state), "attached: image/png 300 bytes", "the image, and not the log");
    assert_eq!(std::fs::read_to_string(state.join("title-prompt")).unwrap(), "say:how do we fix this?");
}

/// A call stuck on a title would otherwise keep the worker running, so archiving waited for the hub to give up on
/// it, and left the call behind.
#[test]
fn archiving_stops_a_title_call_under_way_and_what_it_started() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, chat) = stack.workspace(&client);
    let state = prompt_for_a_held_title(&stack, &client, &chat, "Fix the login redirect");
    let pids: Vec<u32> =
        std::fs::read_to_string(state.join("title-pids")).unwrap().split(' ').map(|pid| pid.parse().unwrap()).collect();

    let stopping = Instant::now();
    client.workspace_archive(&made.id, true).unwrap();

    // Well within the hub's wait for a worker, which a call keeping it running would use up.
    assert!(stopping.elapsed() < Duration::from_secs(8), "took {:?}", stopping.elapsed());
    for pid in pids {
        wait_until("the call to stop", || !is_alive(pid));
    }
    assert_eq!(workspace(&client, &made.id).title_source, TitleSource::FirstLine, "and its title stays");
}
