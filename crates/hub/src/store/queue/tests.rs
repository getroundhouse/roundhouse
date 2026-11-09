use protocol::event::EventBody;
use protocol::model::{ClientMessageId, QueuedPrompt, Session, SessionId};

use crate::store::SessionChange;

use super::*;
use crate::store::tests::store_with_session;

fn queued(id: &str, text: &str) -> QueuedPrompt {
    QueuedPrompt { client_msg_id: id.into(), text: text.into() }
}

fn session(store: &Store) -> Session {
    store.get_session(&"s".into()).unwrap().session
}

fn queue(store: &Store, prompt: &QueuedPrompt) -> Reading {
    let Queued::Done(reading) = store.queue_prompt(&"s".into(), prompt).unwrap() else { panic!("refused") };
    *reading
}

#[test]
fn prompts_queue_in_order_and_a_change_keeps_its_place() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    queue(&store, &queued("a", "first"));
    queue(&store, &queued("b", "second"));
    let changed = queue(&store, &queued("a", "first, changed"));

    assert!(changed.changed);
    assert_eq!(session(&store).queued, [queued("a", "first, changed"), queued("b", "second")]);
    assert_eq!(changed.session.session, session(&store), "it comes back as stored");
    assert_eq!(store.first_queued(&"s".into()).unwrap(), Some(queued("a", "first, changed")));
}

#[test]
fn the_queue_moves_the_revision_only_when_it_changes() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = session(&store).rev;

    let added = queue(&store, &queued("a", "first"));
    let again = queue(&store, &queued("a", "first"));
    let missing = store.unqueue_prompt(&"s".into(), &ClientMessageId::from("nope")).unwrap();

    assert!(added.session.session.rev > before);
    assert!(!again.changed);
    assert!(!missing.changed);
    assert_eq!(session(&store).rev, added.session.session.rev);
}

#[test]
fn a_prompt_taken_out_of_the_queue_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    queue(&store, &queued("a", "first"));
    queue(&store, &queued("b", "second"));

    let removed = store.unqueue_prompt(&"s".into(), &"a".into()).unwrap();

    assert!(removed.changed);
    assert_eq!(session(&store).queued, [queued("b", "second")]);
}

#[test]
fn a_prompt_that_has_been_sent_cant_be_queued_or_changed() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    queue(&store, &queued("a", "first"));
    let prompt = EventBody::UserPrompt {
        text: "first".into(),
        client_msg_id: Some("a".into()),
        attachments: Vec::new(),
        tracked: false,
    };
    store.append(&"s".into(), None, &prompt, None, Some(&"a".into()), None).unwrap();

    let refused = store.queue_prompt(&"s".into(), &queued("a", "changed too late")).unwrap();

    assert!(matches!(refused, Queued::AlreadySent));
    assert!(session(&store).queued.is_empty(), "not put back");
}

#[test]
fn the_hold_commits_with_its_event_and_comes_off_with_another() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = session(&store).rev;
    let hold = |held| Some(SessionChange { status: None, queue_held: Some(held) });

    store.append(&"s".into(), None, &EventBody::Unknown, None, None, hold(true)).unwrap();
    let while_held = session(&store);
    store.append(&"s".into(), None, &EventBody::Unknown, None, None, hold(false)).unwrap();

    assert!(while_held.queue_held && while_held.rev > before, "the record changed, so clients hear of it");
    assert!(!session(&store).queue_held);
}

#[test]
fn a_queued_prompt_can_be_found_by_its_id_until_its_taken_out() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    queue(&store, &queued("a", "first"));
    queue(&store, &queued("b", "second"));

    assert_eq!(store.queued_prompt(&"s".into(), &"b".into()).unwrap(), Some(queued("b", "second")));
    store.unqueue_prompt(&"s".into(), &"b".into()).unwrap();
    assert_eq!(store.queued_prompt(&"s".into(), &"b".into()).unwrap(), None);
}

#[test]
fn recording_a_queued_prompt_takes_it_out_of_the_queue_in_the_same_commit() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    queue(&store, &queued("a", "sent"));
    queue(&store, &queued("b", "still waiting"));
    let before = session(&store).rev;
    let prompt = |id: &str| EventBody::UserPrompt {
        text: "sent".into(),
        client_msg_id: Some(id.into()),
        attachments: Vec::new(),
        tracked: false,
    };

    let sent = store.append(&"s".into(), None, &prompt("a"), None, Some(&"a".into()), None).unwrap();
    let typed = store.append(&"s".into(), None, &prompt("c"), None, Some(&"c".into()), None).unwrap();

    assert!(sent.dequeued && !typed.dequeued, "only a prompt that was queued comes out");
    assert_eq!(session(&store).queued, [queued("b", "still waiting")]);
    assert!(session(&store).rev > before, "the record changed, so clients hear of it");
    assert_eq!(store.sessions_with_queues().unwrap(), [SessionId::from("s")]);
    store.unqueue_prompt(&"s".into(), &"b".into()).unwrap();
    assert!(store.sessions_with_queues().unwrap().is_empty());
}
