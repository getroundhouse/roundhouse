//! What workspaces and chats are called.

#![cfg(test)]

use std::path::Path;
use std::time::{Duration, Instant};

use client_core::{Client, ClientError};
use protocol::client::ErrorCode;
use protocol::model::{AgentKind, Session, TitleSource};

use crate::support::{Stack, is_alive, wait_for_turn_end, wait_until, workspace};

/// Has the fake agent give `title`, and hold the call while `held`, then prompts the chat.
fn prompt_for_a_title(stack: &Stack, client: &Client, chat: &Session, title: &str, held: bool) -> std::path::PathBuf {
    let state = stack.agent_state();
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("title"), title).unwrap();
    if held {
        std::fs::write(state.join("title-held"), "").unwrap();
    }
    client.session_prompt(&chat.id, "say:fix the login\nit loops", None).unwrap();
    if held {
        wait_until("the call for a title", || state.join("title-pids").exists());
    }
    state
}

fn release(state: &Path) {
    std::fs::remove_file(state.join("title-held")).unwrap();
}

#[test]
fn the_first_prompt_titles_the_workspace_and_each_chat() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, first) = stack.workspace(&client);
    assert_eq!((made.title.as_deref(), made.label()), (None, made.name.as_str()), "the name until a prompt");

    client.session_prompt(&first.id, "say:fix the login\nand then the rest", None).unwrap();
    wait_for_turn_end(&client, &first.id, 1);
    let second = client.session_create(&made.id, AgentKind::Claude, None, None, None).unwrap();
    client.session_prompt(&second.id, "say:write the docs", None).unwrap();
    wait_for_turn_end(&client, &second.id, 1);

    let titled = workspace(&client, &made.id);
    assert_eq!((titled.title.as_deref(), titled.title_source), (Some("say:fix the login"), TitleSource::FirstLine));
    assert_eq!((titled.name, titled.branch), (made.name.clone(), made.branch), "a title leaves the name alone");
    assert_eq!(client.session_get(&first.id).unwrap().title.as_deref(), Some("say:fix the login"));
    assert_eq!(client.session_get(&second.id).unwrap().title.as_deref(), Some("say:write the docs"));
}

#[test]
fn a_title_typed_by_hand_stays_through_the_first_prompt() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, chat) = stack.workspace(&client);

    let renamed = client.workspace_rename(&made.id, "  Login\n bug ").unwrap();
    assert_eq!((renamed.title.as_deref(), renamed.title_source), (Some("Login bug"), TitleSource::ByHand));
    let renamed = client.session_rename(&chat.id, "First try").unwrap();
    assert_eq!((renamed.title.as_deref(), renamed.title_source), (Some("First try"), TitleSource::ByHand));
    client.session_prompt(&chat.id, "say:fix the login", None).unwrap();
    wait_for_turn_end(&client, &chat.id, 1);

    assert_eq!(workspace(&client, &made.id).title.as_deref(), Some("Login bug"));
    assert_eq!(client.session_get(&chat.id).unwrap().title.as_deref(), Some("First try"));
    assert!(workspace(&client, &made.id).rev > made.rev);
}

#[test]
fn renaming_to_nothing_or_something_missing_is_refused_and_changes_nothing() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, chat) = stack.workspace(&client);
    client.workspace_rename(&made.id, "Kept").unwrap();
    client.session_rename(&chat.id, "Kept too").unwrap();

    let empty = client.workspace_rename(&made.id, " \n ").unwrap_err();
    let missing_workspace = client.workspace_rename(&"missing".into(), "x").unwrap_err();
    let missing_chat = client.session_rename(&"missing".into(), "x").unwrap_err();

    assert!(matches!(empty, ClientError::Api(e) if e.code == ErrorCode::InvalidRequest));
    assert!(matches!(missing_workspace, ClientError::Api(e) if e.code == ErrorCode::NotFound));
    assert!(matches!(missing_chat, ClientError::Api(e) if e.code == ErrorCode::NotFound));
    assert_eq!(workspace(&client, &made.id).title.as_deref(), Some("Kept"));
    assert_eq!(client.session_get(&chat.id).unwrap().title.as_deref(), Some("Kept too"));
}

#[test]
fn a_generated_title_replaces_the_first_line_of_the_workspace_and_its_chat() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, chat) = stack.workspace(&client);

    let state = prompt_for_a_title(&stack, &client, &chat, "\"Fix the login redirect.\"\n", false);

    wait_until("the chat's generated title", || {
        client.session_get(&chat.id).unwrap().title_source == TitleSource::Generated
    });
    let titled = workspace(&client, &made.id);
    assert_eq!(
        (titled.title.as_deref(), titled.title_source),
        (Some("Fix the login redirect"), TitleSource::Generated)
    );
    assert_eq!(client.session_get(&chat.id).unwrap().title.as_deref(), Some("Fix the login redirect"));
    assert_eq!((titled.name, titled.branch), (made.name, made.branch));
    let asked = std::fs::read_to_string(state.join("title-prompt")).unwrap();
    assert_eq!(asked, "say:fix the login\nit loops", "the whole prompt, not only its first line");
}

#[test]
fn a_workspace_title_typed_while_one_is_generated_stays() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, chat) = stack.workspace(&client);
    let state = prompt_for_a_title(&stack, &client, &chat, "Fix the login redirect", true);

    client.workspace_rename(&made.id, "Login bug").unwrap();
    release(&state);

    // The chat's title is saved after the workspace's, so once it's there, the workspace's is settled.
    wait_until("the chat's generated title", || {
        client.session_get(&chat.id).unwrap().title.as_deref() == Some("Fix the login redirect")
    });
    let kept = workspace(&client, &made.id);
    assert_eq!((kept.title.as_deref(), kept.title_source), (Some("Login bug"), TitleSource::ByHand));
}

#[test]
fn a_chat_title_typed_while_one_is_generated_stays() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, chat) = stack.workspace(&client);
    let state = prompt_for_a_title(&stack, &client, &chat, "Fix the login redirect", true);

    client.session_rename(&chat.id, "First try").unwrap();
    release(&state);

    // Both are saved together, the workspace's first.
    wait_until("the workspace's generated title", || {
        workspace(&client, &made.id).title_source == TitleSource::Generated
    });
    let kept = client.session_get(&chat.id).unwrap();
    assert_eq!((kept.title.as_deref(), kept.title_source), (Some("First try"), TitleSource::ByHand));
}

/// A call stuck on a title would otherwise keep the worker running, so archiving waited for the hub to give up on
/// it, and left the call behind.
#[test]
fn archiving_stops_a_title_call_under_way_and_what_it_started() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, chat) = stack.workspace(&client);
    let state = prompt_for_a_title(&stack, &client, &chat, "Fix the login redirect", true);
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
