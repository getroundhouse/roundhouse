//! Files sent with a prompt, from the client to the agent, and back to anyone reading the chat.

#![cfg(test)]

use std::path::PathBuf;

use client_core::{Client, ClientError};
use protocol::attachment::{Attachment, MediaType};
use protocol::client::ErrorCode;
use protocol::event::EventBody;
use protocol::model::{AttachmentId, SessionId};

use crate::support::{Stack, assistant_text, git, read_all, wait_for_turn_end};

/// Enough of a PNG for anything that only looks at how it starts, which is all the hub and the fake agent do.
fn png(len: usize) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.resize(len, 7);
    bytes
}

fn add(client: &Client, session: &SessionId, id: &str, name: &str, media_type: MediaType, data: &[u8]) -> Attachment {
    client.attachment_add(session, &AttachmentId::from(id), name, media_type, data.to_vec()).unwrap()
}

fn refused_with(result: Result<impl std::fmt::Debug, ClientError>) -> ErrorCode {
    match result {
        Err(ClientError::Api(e)) => e.code,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn every_file_goes_in_the_worktree_and_the_agent_is_given_what_it_can_take_in_the_prompt() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.unwrap());
    let shot = add(&client, &session.id, "a1", "/Users/someone/Desktop/shot.png", MediaType::Png, &png(300));
    let log = add(&client, &session.id, "a2", "build.log", MediaType::Text, b"error at line 42\nmore");
    let archive = add(&client, &session.id, "a3", "logs.zip", MediaType::Other, b"PK\x03\x04");
    let long_log = add(&client, &session.id, "a4", "long.log", MediaType::Text, &vec![b'a'; 40_000]);

    let carried = vec![shot.clone(), log.clone(), archive, long_log];
    let ids = carried.iter().map(|a| a.id.clone()).collect();
    client.session_prompt_with(&session.id, "attached", None, ids).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_eq!(
        assistant_text(&events),
        "attached: image/png 300 bytes | shot.png: on disk, 300 bytes | build.log: error at line 42 \
         | logs.zip: on disk, 4 bytes | long.log: on disk, 40000 bytes"
    );
    let recorded = events.iter().find_map(|e| match &e.body {
        EventBody::UserPrompt { attachments, .. } => Some(attachments.clone()),
        _ => None,
    });
    assert_eq!(recorded, Some(carried));
    assert_eq!(shot.name, "shot.png", "only the file's own name is kept");
    let folder = worktree.join(".context/attachments");
    assert_eq!(std::fs::read(folder.join("build.log")).unwrap(), b"error at line 42\nmore");
    assert_eq!(std::fs::read(folder.join("shot.png")).unwrap(), png(300));
    assert_eq!(git(&worktree, &["status", "--porcelain", "--untracked-files=all"]), "", "git never sees them");
}

#[test]
fn the_hub_keeps_an_image_once_its_sent_and_lets_go_of_anything_else() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let shot = add(&client, &session.id, "a1", "shot.png", MediaType::Png, &png(64));
    let log = add(&client, &session.id, "a2", "build.log", MediaType::Text, b"error at line 42");
    assert_eq!(client.attachment_read(&session.id, &log.id).unwrap(), b"error at line 42", "until it's sent");

    client.session_prompt_with(&session.id, "attached", None, vec![shot.id.clone(), log.id.clone()]).unwrap();
    let first = assistant_text(&wait_for_turn_end(&client, &session.id, 1));

    assert_eq!(client.attachment_read(&session.id, &shot.id).unwrap(), png(64));
    assert_eq!(refused_with(client.attachment_read(&session.id, &log.id)), ErrorCode::NotFound);
    // A later prompt can carry it again, from where it went.
    client.session_prompt_with(&session.id, "attached", None, vec![log.id.clone()]).unwrap();
    let both = assistant_text(&wait_for_turn_end(&client, &session.id, 2));
    assert_eq!(both.strip_prefix(&first), Some("attached: build.log: error at line 42"));
}

#[test]
fn a_prompt_can_be_only_files() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let shot = add(&client, &session.id, "a1", "shot.png", MediaType::Png, &png(64));

    client.session_prompt_with(&session.id, "", None, vec![shot.id.clone()]).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(events.iter().any(
        |e| matches!(&e.body, EventBody::UserPrompt { attachments, .. } if attachments == std::slice::from_ref(&shot))
    ));
    let refused = client.session_prompt_with(&session.id, " ", None, Vec::new());
    assert_eq!(refused_with(refused), ErrorCode::InvalidRequest, "with no files, it still needs text");
}

#[test]
fn adding_the_same_attachment_again_keeps_the_first() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let first = add(&client, &session.id, "a1", "one.log", MediaType::Text, b"one");

    let again = add(&client, &session.id, "a1", "two.log", MediaType::Text, b"two");

    assert_eq!(again, first);
    assert_eq!(client.attachment_read(&session.id, &first.id).unwrap(), b"one");
}

#[test]
fn a_chat_refuses_a_file_that_isnt_what_it_says_and_records_nothing() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let id = AttachmentId::from("a1");

    let renamed = client.attachment_add(&session.id, &id, "shot.png", MediaType::Png, vec![0xff, 0xd8, 0xff, 0xe0]);
    let not_text = client.attachment_add(&session.id, &id, "a.bin", MediaType::Text, vec![0, 1, 2]);
    let empty = client.attachment_add(&session.id, &id, "empty.log", MediaType::Text, Vec::new());
    let unknown = client.attachment_add(&session.id, &id, "a.pdf", MediaType::Unknown, b"%PDF-1.4".to_vec());
    let climbing = client.attachment_add(&session.id, &"../../hub".into(), "a.log", MediaType::Text, b"x".to_vec());
    let shouting = client.attachment_add(&session.id, &"A1".into(), "a.log", MediaType::Text, b"x".to_vec());

    for refused in [renamed, not_text, empty, climbing, shouting] {
        assert_eq!(refused_with(refused), ErrorCode::InvalidRequest);
    }
    assert_eq!(refused_with(unknown), ErrorCode::Unsupported, "so a newer client can send it as a plain file");
    assert_eq!(refused_with(client.attachment_read(&session.id, &id)), ErrorCode::NotFound);
}

#[test]
fn a_prompt_can_only_carry_what_was_added_to_its_own_chat() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let other = client.session_create(&workspace.id, protocol::model::AgentKind::Claude, None, None, None).unwrap();
    let shot = add(&client, &other.id, "a1", "shot.png", MediaType::Png, &png(64));

    let elsewhere = client.session_prompt_with(&session.id, "attached", None, vec![shot.id.clone()]);
    let too_many = client.session_prompt_with(&other.id, "attached", None, vec![shot.id.clone(); 11]);

    assert_eq!(refused_with(elsewhere), ErrorCode::NotFound);
    assert_eq!(refused_with(too_many), ErrorCode::InvalidRequest);
    assert_eq!(refused_with(client.attachment_read(&session.id, &shot.id)), ErrorCode::NotFound);
    assert!(read_all(&client, &session.id).is_empty(), "nothing is recorded for a refused prompt");
}
