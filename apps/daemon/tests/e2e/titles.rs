//! What workspaces and chats are called.

#![cfg(test)]

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use client_core::{Client, ClientError};
use protocol::attachment::{Attachment, MediaType};
use protocol::client::ErrorCode;
use protocol::model::{AgentKind, AttachmentId, Session, SessionSettingsChange, TitleSource};

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

#[test]
fn the_first_prompt_titles_the_workspace_and_each_chat() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, first) = stack.workspace(&client);
    assert_eq!((made.title.as_deref(), made.label()), (None, made.name.as_str()), "the name until a prompt");

    client.session_prompt(&first.id, "say:fix the login\nand then the rest", None).unwrap();
    wait_for_turn_end(&client, &first.id, 1);
    let second = client.session_create(&made.id, AgentKind::Claude, SessionSettingsChange::default(), None).unwrap();
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

    client.session_prompt_with(&chat.id, "say:how do we fix this?", None, carried).unwrap();

    wait_until("the chat's generated title", || {
        client.session_get(&chat.id).unwrap().title_source == TitleSource::Generated
    });
    assert_eq!(workspace(&client, &made.id).title.as_deref(), Some("Fix the queue error"));
    assert_eq!(given_with(&state), "attached: image/png 300 bytes", "the image, and not the log");
    assert_eq!(std::fs::read_to_string(state.join("title-prompt")).unwrap(), "say:how do we fix this?");
}

#[test]
fn a_workspaces_images_go_with_its_prompts_for_a_new_title() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, chat) = stack.workspace(&client);
    let state = ready_to_title(&stack);
    let shot = add(&client, &chat, "shot.png", MediaType::Png);
    client.session_prompt_with(&chat.id, "say:how do we fix this?", None, vec![shot.id]).unwrap();
    wait_until("the chat's generated title", || {
        client.session_get(&chat.id).unwrap().title_source == TitleSource::Generated
    });
    wait_for_turn_end(&client, &chat.id, 1);
    std::fs::remove_file(state.join("title-images")).unwrap();

    client.workspace_regenerate_title(&made.id).unwrap();

    assert_eq!(given_with(&state), "attached: image/png 300 bytes");
}

/// With no words there's no first line, so nothing is shown in its place while the title is on its way.
#[test]
fn a_prompt_of_only_an_image_is_titled_once_its_generated_title_comes() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, chat) = stack.workspace(&client);
    let state = ready_to_title(&stack);
    std::fs::write(state.join("title-held"), "").unwrap();
    let shot = add(&client, &chat, "shot.png", MediaType::Png);

    client.session_prompt_with(&chat.id, "", None, vec![shot.id]).unwrap();
    wait_until("the call for a title", || state.join("title-pids").exists());
    let meanwhile = (workspace(&client, &made.id).title, client.session_get(&chat.id).unwrap().title);
    release(&state);

    assert_eq!(meanwhile, (None, None));
    // The chat's title is saved after the workspace's, so once it's there, the workspace's is settled.
    wait_until("the chat's generated title", || {
        client.session_get(&chat.id).unwrap().title.as_deref() == Some("Fix the queue error")
    });
    let titled = workspace(&client, &made.id);
    assert_eq!((titled.title.as_deref(), titled.title_source), (Some("Fix the queue error"), TitleSource::Generated));
    assert_eq!(given_with(&state), "attached: image/png 300 bytes");
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

/// A workspace with a generated title and two prompts, the second in a chat of its own, and what the fake agent keeps.
fn worked_in(stack: &Stack, client: &Client) -> (protocol::model::Workspace, Session, std::path::PathBuf) {
    let (made, chat) = stack.workspace(client);
    let state = prompt_for_a_title(stack, client, &chat, "Fix the login redirect", false);
    wait_until("the chat's generated title", || {
        client.session_get(&chat.id).unwrap().title_source == TitleSource::Generated
    });
    wait_for_turn_end(client, &chat.id, 1);
    let second = client.session_create(&made.id, AgentKind::Claude, SessionSettingsChange::default(), None).unwrap();
    client.session_prompt(&second.id, "say:now add a test", None).unwrap();
    wait_for_turn_end(client, &second.id, 1);
    // Its own first prompt asks for a title too, which would otherwise still be on its way when a test holds the next.
    wait_until("the second chat's generated title", || {
        client.session_get(&second.id).unwrap().title_source == TitleSource::Generated
    });
    (made, chat, state)
}

#[test]
fn asking_for_a_new_title_names_the_workspace_from_all_its_prompts_even_over_one_typed_by_hand() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, chat, state) = worked_in(&stack, &client);
    client.workspace_rename(&made.id, "Login bug").unwrap();
    std::fs::write(state.join("title"), "Fix and test the login").unwrap();

    let retitled = client.workspace_regenerate_title(&made.id).unwrap();

    let now = (retitled.title.as_deref(), retitled.title_source);
    assert_eq!(now, (Some("Fix and test the login"), TitleSource::Generated));
    assert_eq!(workspace(&client, &made.id), retitled, "what it answers is what's kept");
    assert_eq!((retitled.name, retitled.branch), (made.name, made.branch), "a title leaves the name alone");
    let asked = std::fs::read_to_string(state.join("title-prompt")).unwrap();
    let prompts = "\n\n1. say:fix the login it loops\n2. say:now add a test";
    assert!(asked.ends_with(prompts), "every chat's prompts, oldest first: {asked}");
    assert_eq!(client.session_get(&chat.id).unwrap().title.as_deref(), Some("Fix the login redirect"), "not its chats");
}

#[test]
fn a_title_typed_while_a_new_one_is_asked_for_stays() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, _, state) = worked_in(&stack, &client);
    std::fs::write(state.join("title"), "Fix and test the login").unwrap();
    std::fs::write(state.join("title-held"), "").unwrap();
    let (asking, id) = (stack.client(), made.id.clone());
    let asked = std::thread::spawn(move || asking.workspace_regenerate_title(&id));
    wait_until("the call for a title", || state.join("title-pids").exists());

    client.workspace_rename(&made.id, "Login bug").unwrap();
    release(&state);

    let answered = asked.join().unwrap().unwrap();
    assert_eq!((answered.title.as_deref(), answered.title_source), (Some("Login bug"), TitleSource::ByHand));
    assert_eq!(workspace(&client, &made.id).title.as_deref(), Some("Login bug"));
}

#[test]
fn asking_for_a_new_title_is_refused_with_nothing_to_go_on_and_says_when_none_comes() {
    let stack = Stack::start();
    let client = stack.client();
    let (made, chat) = stack.workspace(&client);

    let unprompted = client.workspace_regenerate_title(&made.id).unwrap_err();
    let missing = client.workspace_regenerate_title(&"missing".into()).unwrap_err();
    // The fake agent has no title to give, as when the CLI isn't logged in.
    client.session_prompt(&chat.id, "say:fix the login", None).unwrap();
    wait_for_turn_end(&client, &chat.id, 1);
    let failed = client.workspace_regenerate_title(&made.id).unwrap_err();
    client.workspace_archive(&made.id, true).unwrap();
    let hub = stack.hub_pid().to_string();
    let workers = || String::from_utf8(Command::new("pgrep").args(["-P", &hub]).output().unwrap().stdout).unwrap();
    let before = workers();
    let archived = client.workspace_regenerate_title(&made.id).unwrap_err();

    assert!(matches!(archived, ClientError::Api(e) if e.code == ErrorCode::Conflict));
    assert_eq!(workers(), before, "and starts no worker for it");
    assert!(matches!(unprompted, ClientError::Api(e) if e.code == ErrorCode::Conflict));
    assert!(matches!(missing, ClientError::Api(e) if e.code == ErrorCode::NotFound));
    assert!(matches!(failed, ClientError::Api(e) if e.code == ErrorCode::Worker));
    let kept = workspace(&client, &made.id);
    assert_eq!((kept.title.as_deref(), kept.title_source), (Some("say:fix the login"), TitleSource::FirstLine));
}
