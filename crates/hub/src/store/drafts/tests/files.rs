//! The files a composer's draft names, and letting go of those nothing will send.

use protocol::attachment::{Attachment, MAX_PER_PROMPT, MediaType};
use protocol::event::EventBody;
use protocol::model::{AttachmentId, Draft, DraftToKeep, QueuedPrompt};
use protocol::prompt::{Prompt, PromptFrom};

use super::id;
use crate::store::tests::store_with_session;
use crate::store::{AddedBy, Queued};

fn log_file(id: &str, name: &str) -> Attachment {
    Attachment { id: AttachmentId::from(id), name: name.to_string(), media_type: MediaType::Text, size_bytes: 3 }
}

/// A draft with `files`, as a client names them, by ID.
fn with_files(files: &[Attachment]) -> DraftToKeep {
    DraftToKeep { text: "see these".to_string(), files: files.iter().map(|file| file.id.clone()).collect() }
}

#[test]
fn a_drafts_files_are_named_as_the_session_has_them_and_one_it_hasnt_been_given_is_left_out() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_attachment(&id(), &log_file("a1", "build.log"), AddedBy::Client).unwrap();

    let kept =
        store.set_draft(&id(), &with_files(&[log_file("a1", "build.log"), log_file("a2", "never.log")]), None).unwrap();

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

    let kept = store.set_draft(&id(), &with_files(&files), None).unwrap();

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
        client_msg_id: None,
        prompt: Prompt::carrying("see this", vec![log_file(file, "build.log")]),
        from: PromptFrom::Person,
    }
}

#[test]
fn a_file_the_draft_a_queued_prompt_or_a_sent_one_names_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    add_files(&store, &["drafted", "queued", "sent", "placed"]);
    store.set_draft(&id(), &with_files(&[log_file("drafted", "build.log")]), None).unwrap();
    let waiting = QueuedPrompt {
        client_msg_id: "q".into(),
        prompt: Prompt::carrying("", vec![log_file("queued", "build.log")]),
        from: PromptFrom::Person,
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
        .set_draft(&id(), &with_files(&[log_file("drafted", "build.log"), log_file("also sent", "build.log")]), None)
        .unwrap();
    store.append(&id(), None, &prompt_carrying("also sent"), None, None, None).unwrap();

    let let_go_of = store.set_session_closed(&id(), Some(1)).unwrap();

    assert_eq!(let_go_of, [AttachmentId::from("drafted")]);
    assert_eq!(store.get_session(&id()).unwrap().session.draft, Draft::default());
    assert!(store.attachment(&id(), &"also sent".into()).unwrap().is_some());
    let typed_late = store.set_draft(&id(), &DraftToKeep::new("typed as it closed"), None).unwrap();
    assert!(!typed_late.changed, "a closed session keeps none");
    assert_eq!(store.set_session_closed(&id(), None).unwrap(), [], "and none comes back as it opens again");
    assert_eq!(store.get_session(&id()).unwrap().session.draft, Draft::default());
}
