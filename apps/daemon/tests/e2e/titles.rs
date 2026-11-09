//! What workspaces and chats are called.

#![cfg(test)]

use client_core::ClientError;
use protocol::client::ErrorCode;
use protocol::model::{AgentKind, TitleSource};

use crate::support::{Stack, wait_for_turn_end, workspace};

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
