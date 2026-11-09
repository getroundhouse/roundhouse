use protocol::attachment::{Attachment, MediaType};
use protocol::model::AttachmentId;

use super::super::tests::store_with_session;
use super::*;

fn log_file(id: &str, name: &str) -> Attachment {
    Attachment { id: AttachmentId::from(id), name: name.to_string(), media_type: MediaType::Text, size_bytes: 3 }
}

#[test]
fn an_attachment_keeps_what_it_was_given() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let session = SessionId::from("s");

    let added = store.add_attachment(&session, &log_file("a1", "build.log")).unwrap();

    assert_eq!(added, log_file("a1", "build.log"));
    assert_eq!(store.attachment(&session, &"a1".into()).unwrap(), Some(added));
}

#[test]
fn adding_the_same_id_again_keeps_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let session = SessionId::from("s");
    store.add_attachment(&session, &log_file("a1", "build.log")).unwrap();

    let again = store.add_attachment(&session, &log_file("a1", "other.log")).unwrap();

    assert_eq!(again.name, "build.log");
}

#[test]
fn attachments_come_back_in_the_order_asked_and_a_missing_one_fails() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let session = SessionId::from("s");
    store.add_attachment(&session, &log_file("a1", "one.log")).unwrap();
    store.add_attachment(&session, &log_file("a2", "two.log")).unwrap();

    let found = store.attachments(&session, &["a2".into(), "a1".into()]).unwrap();
    let missing = store.attachments(&session, &["a1".into(), "a3".into()]);

    assert_eq!(found.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["two.log", "one.log"]);
    assert!(matches!(missing, Err(StoreError::NotFound { kind: "attachment", .. })), "{missing:?}");
}

#[test]
fn another_session_doesnt_have_an_attachment() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_attachment(&"s".into(), &log_file("a1", "build.log")).unwrap();

    assert_eq!(store.attachment(&"t".into(), &"a1".into()).unwrap(), None);
}

#[test]
fn where_an_attachment_was_put_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let session = SessionId::from("s");
    store.add_attachment(&session, &log_file("a1", "build.log")).unwrap();
    assert_eq!(store.attachment_placed_at(&session, &"a1".into()).unwrap(), None);

    store.set_attachment_placed_at(&session, &"a1".into(), "/w/.context/attachments/build.log").unwrap();

    let placed = store.attachment_placed_at(&session, &"a1".into()).unwrap();
    assert_eq!(placed.as_deref(), Some("/w/.context/attachments/build.log"));
    assert!(matches!(store.set_attachment_placed_at(&session, &"a2".into(), "/x"), Err(StoreError::NotFound { .. })));
}
