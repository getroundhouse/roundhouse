use protocol::attachment::{Attachment, MediaType};
use protocol::event::EventBody;
use protocol::model::AttachmentId;
use protocol::prompt::Prompt;

use super::super::tests::{idle_session, store_with_session};
use super::*;

fn log_file(id: &str, name: &str) -> Attachment {
    Attachment { id: AttachmentId::from(id), name: name.to_string(), media_type: MediaType::Text, size_bytes: 3 }
}

#[test]
fn an_attachment_keeps_what_it_was_given() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let session = SessionId::from("s");

    let added = store.add_attachment(&session, &log_file("a1", "build.log"), AddedBy::Client).unwrap();

    assert_eq!(added, log_file("a1", "build.log"));
    assert_eq!(store.attachment(&session, &"a1".into()).unwrap(), Some(added));
}

#[test]
fn adding_the_same_id_again_keeps_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let session = SessionId::from("s");
    store.add_attachment(&session, &log_file("a1", "build.log"), AddedBy::Client).unwrap();

    let again = store.add_attachment(&session, &log_file("a1", "other.log"), AddedBy::Client).unwrap();

    assert_eq!(again.name, "build.log");
}

#[test]
fn attachments_come_back_in_the_order_asked_and_a_missing_one_fails() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let session = SessionId::from("s");
    store.add_attachment(&session, &log_file("a1", "one.log"), AddedBy::Client).unwrap();
    store.add_attachment(&session, &log_file("a2", "two.log"), AddedBy::Client).unwrap();

    let found = store.attachments(&session, &["a2".into(), "a1".into()]).unwrap();
    let missing = store.attachments(&session, &["a1".into(), "a3".into()]);

    assert_eq!(found.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["two.log", "one.log"]);
    assert!(matches!(missing, Err(StoreError::NotFound { kind: "attachment", .. })), "{missing:?}");
}

#[test]
fn another_session_doesnt_have_an_attachment() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_attachment(&"s".into(), &log_file("a1", "build.log"), AddedBy::Client).unwrap();

    assert_eq!(store.attachment(&"t".into(), &"a1".into()).unwrap(), None);
}

#[test]
fn where_an_attachment_was_put_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let session = SessionId::from("s");
    store.add_attachment(&session, &log_file("a1", "build.log"), AddedBy::Client).unwrap();
    assert_eq!(store.attachment_placed_at(&session, &"a1".into()).unwrap(), None);

    store.set_attachment_placed_at(&session, &"a1".into(), "/w/.context/attachments/build.log").unwrap();

    let placed = store.attachment_placed_at(&session, &"a1".into()).unwrap();
    assert_eq!(placed.as_deref(), Some("/w/.context/attachments/build.log"));
    assert!(matches!(store.set_attachment_placed_at(&session, &"a2".into(), "/x"), Err(StoreError::NotFound { .. })));
}

#[test]
fn a_file_for_a_session_thats_been_deleted_is_refused_as_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.delete_unused_session(&"s".into()).unwrap().unwrap();

    let added = store.add_attachment(&"s".into(), &log_file("a1", "build.log"), AddedBy::Client);

    assert!(matches!(added, Err(StoreError::NotFound { .. })), "{added:?}");
}

/// A title is asked from the images a workspace's prompts carried, so they're read across its chats, in the order the
/// prompts were sent, and only images.
#[test]
fn a_workspaces_prompt_images_are_listed_oldest_first_across_its_chats() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut second = idle_session(1);
    (second.session.id, second.agent_session_id) = ("second".into(), "second".into());
    store.insert_session(&second, None).unwrap();
    let image = |id: &str| Attachment { media_type: MediaType::Png, ..log_file(id, &format!("{id}.png")) };
    let sent = |chat: &str, at_ms: i64, attachments: Vec<Attachment>| {
        let body = EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::carrying("look", attachments) };
        let seq = store.append(&chat.into(), None, &body, None, None, None).unwrap().event.seq;
        let when = "UPDATE events SET ts_ms = ?1 WHERE session_id = ?2 AND seq = ?3";
        lock(&store.writer).execute(when, params![at_ms, chat, seq.cast_signed()]).unwrap();
    };

    sent("s", 10, vec![image("a1"), log_file("a2", "build.log"), image("a3")]);
    sent("second", 20, vec![]);
    sent("s", 40, vec![image("a4")]);
    sent("second", 30, vec![image("b1")]);

    let listed = store.workspace_prompt_images(&"w".into()).unwrap();

    let found: Vec<(&str, &str)> = listed.iter().map(|(chat, a)| (chat.as_str(), a.id.as_str())).collect();
    assert_eq!(found, [("s", "a1"), ("s", "a3"), ("second", "b1"), ("s", "a4")]);
    assert_eq!(listed[0].1, image("a1"), "each as its prompt says it is");
    assert!(store.workspace_prompt_images(&"missing".into()).unwrap().is_empty());
}
