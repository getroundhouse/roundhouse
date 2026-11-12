use protocol::attachment::{Attachment, MAX_PER_PROMPT, MediaType};
use protocol::event::EventBody;
use protocol::model::{AttachmentId, Draft, NewDraft, QueuedPrompt, SessionId};

use crate::store::tests::{open_in, store_with_session};
use crate::store::{AddedBy, Queued, StoreError};

fn id() -> SessionId {
    "s".into()
}

#[test]
fn a_draft_is_kept_until_another_replaces_it_and_an_empty_one_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert_eq!(store.get_session(&id()).unwrap().session.draft, Draft::default(), "nothing's typed yet");

    let kept = store.set_draft(&id(), &NewDraft::new("half a thought")).unwrap();

    assert!(kept.changed);
    assert_eq!(kept.session.session.draft.text, "half a thought");
    assert_eq!(kept.session.session, store.get_session(&id()).unwrap().session, "it comes back as stored");
    store.set_draft(&id(), &NewDraft::new("half a thought, and more")).unwrap();
    assert_eq!(store.get_session(&id()).unwrap().session.draft.text, "half a thought, and more");
    let cleared = store.set_draft(&id(), &NewDraft::default()).unwrap();
    assert!(cleared.changed);
    assert_eq!(store.get_session(&id()).unwrap().session.draft, Draft::default());
}

#[test]
fn keeping_the_same_draft_again_leaves_the_revision_alone() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let first = store.set_draft(&id(), &NewDraft::new("same")).unwrap().session.session.rev;

    let again = store.set_draft(&id(), &NewDraft::new("same")).unwrap();
    let cleared_twice = [NewDraft::default(), NewDraft::default()].map(|d| store.set_draft(&id(), &d).unwrap().changed);

    assert!(!again.changed, "nothing changed, so nothing is announced");
    assert_eq!(again.session.session.rev, first);
    assert_eq!(cleared_twice, [true, false]);
}

#[test]
fn a_change_to_the_draft_gives_the_session_a_newer_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_session(&id()).unwrap().session.rev;

    let after = store.set_draft(&id(), &NewDraft::new("typed")).unwrap().session.session.rev;

    assert!(after > before, "clients follow records by revision, so the feed would miss it otherwise");
}

#[test]
fn a_draft_for_a_session_that_isnt_there_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());

    assert!(matches!(
        store.set_draft(&id(), &NewDraft::new("typed")),
        Err(StoreError::NotFound { kind: "session", .. })
    ));
}

fn log_file(id: &str, name: &str) -> Attachment {
    Attachment { id: AttachmentId::from(id), name: name.to_string(), media_type: MediaType::Text, size_bytes: 3 }
}

/// A draft with `files`, as a client names them, by ID.
fn with_files(files: &[Attachment]) -> NewDraft {
    NewDraft { text: "see these".to_string(), files: files.iter().map(|file| file.id.clone()).collect() }
}

#[test]
fn a_drafts_files_are_named_as_the_session_has_them_and_one_it_hasnt_been_given_is_left_out() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_attachment(&id(), &log_file("a1", "build.log"), AddedBy::Client).unwrap();

    let kept =
        store.set_draft(&id(), &with_files(&[log_file("a1", "build.log"), log_file("a2", "never.log")])).unwrap();

    assert_eq!(kept.session.session.draft.files, [log_file("a1", "build.log")]);
    assert_eq!(store.get_session(&id()).unwrap().session.draft.files, [log_file("a1", "build.log")]);
}

#[test]
fn a_file_nothing_will_send_is_let_go_of() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_attachment(&id(), &log_file("a1", "build.log"), AddedBy::Client).unwrap();

    assert!(store.let_go_of_attachment(&id(), &"a1".into()).unwrap());

    assert_eq!(store.attachment(&id(), &"a1".into()).unwrap(), None);
    assert!(!store.let_go_of_attachment(&id(), &"a1".into()).unwrap(), "letting go again does nothing more");
}

#[test]
fn a_draft_names_no_more_files_than_a_prompt_can_carry() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let files: Vec<Attachment> = (0..=MAX_PER_PROMPT).map(|n| log_file(&format!("a{n}"), "build.log")).collect();
    for file in &files {
        store.add_attachment(&id(), file, AddedBy::Client).unwrap();
    }

    let kept = store.set_draft(&id(), &with_files(&files)).unwrap();

    assert_eq!(kept.session.session.draft.files, files[..MAX_PER_PROMPT]);
}

/// Adds a file for each of `ids`, as a client does.
fn add_files(store: &crate::store::Store, ids: &[&str]) {
    for file in ids {
        store.add_attachment(&id(), &log_file(file, "build.log"), AddedBy::Client).unwrap();
    }
}

fn prompt_carrying(file: &str) -> EventBody {
    EventBody::UserPrompt {
        text: "see this".to_string(),
        client_msg_id: None,
        attachments: vec![log_file(file, "build.log")],
    }
}

#[test]
fn a_file_the_draft_a_queued_prompt_or_a_sent_one_names_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    add_files(&store, &["drafted", "queued", "sent", "placed"]);
    store.set_draft(&id(), &with_files(&[log_file("drafted", "build.log")])).unwrap();
    let waiting = QueuedPrompt {
        client_msg_id: "q".into(),
        text: String::new(),
        attachments: vec![log_file("queued", "build.log")],
    };
    assert!(matches!(store.queue_prompt(&id(), &waiting).unwrap(), Queued::Done(_)));
    store.append(&id(), None, &prompt_carrying("sent"), None, None, None).unwrap();
    store.set_attachment_placed_at(&id(), &"placed".into(), ".context/attachments/build.log").unwrap();

    let let_go =
        ["drafted", "queued", "sent", "placed"].map(|file| store.let_go_of_attachment(&id(), &file.into()).unwrap());

    assert_eq!(let_go, [false; 4]);
    assert!(store.attachment(&id(), &"sent".into()).unwrap().is_some());
}

#[test]
fn an_image_from_the_agent_is_never_let_go_of() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let screenshot = Attachment { media_type: MediaType::Png, ..log_file("shot", "screenshot.png") };
    store.add_attachment(&id(), &screenshot, AddedBy::Agent).unwrap();

    assert!(!store.let_go_of_attachment(&id(), &"shot".into()).unwrap(), "only a client's file can be in a draft");
    assert!(store.attachment(&id(), &"shot".into()).unwrap().is_some());
}

#[test]
fn closing_a_session_takes_its_draft_and_lets_go_of_the_files_only_the_draft_named() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    add_files(&store, &["drafted", "also sent"]);
    store
        .set_draft(&id(), &with_files(&[log_file("drafted", "build.log"), log_file("also sent", "build.log")]))
        .unwrap();
    store.append(&id(), None, &prompt_carrying("also sent"), None, None, None).unwrap();

    let let_go_of = store.set_session_closed(&id(), Some(1)).unwrap();

    assert_eq!(let_go_of, [AttachmentId::from("drafted")]);
    assert_eq!(store.get_session(&id()).unwrap().session.draft, Draft::default());
    assert!(store.attachment(&id(), &"also sent".into()).unwrap().is_some());
    let typed_late = store.set_draft(&id(), &NewDraft::new("typed as it closed")).unwrap();
    assert!(!typed_late.changed, "a closed session keeps none");
    assert_eq!(store.set_session_closed(&id(), None).unwrap(), [], "and none comes back as it opens again");
    assert_eq!(store.get_session(&id()).unwrap().session.draft, Draft::default());
}
