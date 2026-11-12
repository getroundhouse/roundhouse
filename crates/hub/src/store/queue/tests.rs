use protocol::attachment::{Attachment, MediaType};
use protocol::event::EventBody;
use protocol::model::{ClientMessageId, QueuedPrompt, Session, SessionId};
use protocol::prompt::Prompt;

use crate::store::SessionChange;

use super::*;
use crate::store::tests::store_with_session;

fn queued(id: &str, text: &str) -> QueuedPrompt {
    QueuedPrompt { client_msg_id: id.into(), prompt: Prompt::new(text) }
}

fn log(id: &str) -> Attachment {
    Attachment { id: id.into(), name: format!("{id}.log"), media_type: MediaType::Text, size_bytes: 3 }
}

fn session(store: &Store) -> Session {
    store.get_session(&"s".into()).unwrap().session
}

fn queue(store: &Store, prompt: &QueuedPrompt) -> Reading {
    let Queued::Done(reading) = store.queue_prompt(&"s".into(), prompt).unwrap() else { panic!("refused") };
    *reading
}

fn change(store: &Store, id: &str, text: &str) -> Changed {
    store.change_queued(&"s".into(), &id.into(), text).unwrap()
}

fn changed(store: &Store, id: &str, text: &str) -> Reading {
    let Changed::Done(reading) = change(store, id, text) else { panic!("refused") };
    *reading
}

#[test]
fn prompts_queue_in_order_and_a_change_keeps_its_place() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    queue(&store, &queued("a", "first"));
    queue(&store, &queued("b", "second"));
    let changed = changed(&store, "a", "first, changed");

    assert!(changed.changed);
    assert_eq!(session(&store).queued, [queued("a", "first, changed"), queued("b", "second")]);
    assert_eq!(changed.session.session, session(&store), "it comes back as stored");
    assert_eq!(store.first_queued(&"s".into()).unwrap(), Some(queued("a", "first, changed")));
}

#[test]
fn queueing_a_prompt_again_is_a_retry_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    queue(&store, &queued("a", "first"));

    let again = queue(&store, &queued("a", "not a change"));

    assert!(!again.changed);
    assert_eq!(session(&store).queued, [queued("a", "first")]);
}

#[test]
fn a_change_keeps_the_files_it_was_queued_with_and_can_leave_only_them() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let carrying = QueuedPrompt {
        client_msg_id: "a".into(),
        prompt: Prompt::carrying("look at these", vec![log("a1"), log("a2")]),
    };
    queue(&store, &carrying);
    queue(&store, &queued("b", "no files"));

    changed(&store, "a", "look at these, changed");
    let files_only = changed(&store, "a", " ");
    let emptied = change(&store, "b", " ");

    let kept = QueuedPrompt { prompt: Prompt::carrying(" ", carrying.prompt.files.clone()), ..carrying };
    assert_eq!(files_only.session.session.queued, [kept.clone(), queued("b", "no files")]);
    assert!(matches!(emptied, Changed::WouldBeEmpty));
    assert_eq!(session(&store).queued, [kept, queued("b", "no files")], "the refused change wasn't kept");
}

#[test]
fn a_change_to_a_prompt_taken_out_is_refused_rather_than_queued() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    queue(&store, &queued("a", "first"));
    store.unqueue_prompt(&"s".into(), &"a".into()).unwrap();

    let late = change(&store, "a", "changed too late");

    assert!(matches!(late, Changed::NotQueued));
    assert!(session(&store).queued.is_empty());
}

#[test]
fn a_queue_kept_before_prompts_carried_files_reads_as_carrying_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let older = r#"[{"client_msg_id":"a","text":"first"}]"#;
    lock(&store.writer).execute("UPDATE sessions SET queued = ? WHERE id = 's'", [older]).unwrap();

    assert_eq!(session(&store).queued, [queued("a", "first")]);
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
    let prompt = EventBody::UserPrompt { client_msg_id: Some("a".into()), prompt: Prompt::new("first") };
    store.append(&"s".into(), None, &prompt, None, Some(&"a".into()), None).unwrap();

    let refused = store.queue_prompt(&"s".into(), &queued("a", "queued again too late")).unwrap();
    let late = change(&store, "a", "changed too late");

    assert!(matches!(refused, Queued::AlreadySent));
    assert!(matches!(late, Changed::AlreadySent));
    assert!(session(&store).queued.is_empty(), "not put back");
}

#[test]
fn the_hold_commits_with_its_event_and_comes_off_with_another() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = session(&store).rev;
    let hold = |held| Some(SessionChange { queue_held: Some(held), ..SessionChange::default() });

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
    let prompt = |id: &str| EventBody::UserPrompt { client_msg_id: Some(id.into()), prompt: Prompt::new("sent") };

    let sent = store.append(&"s".into(), None, &prompt("a"), None, Some(&"a".into()), None).unwrap();
    let typed = store.append(&"s".into(), None, &prompt("c"), None, Some(&"c".into()), None).unwrap();

    assert!(sent.dequeued && !typed.dequeued, "only a prompt that was queued comes out");
    assert_eq!(session(&store).queued, [queued("b", "still waiting")]);
    assert!(session(&store).rev > before, "the record changed, so clients hear of it");
    assert_eq!(store.sessions_with_queues().unwrap(), [SessionId::from("s")]);
    store.unqueue_prompt(&"s".into(), &"b".into()).unwrap();
    assert!(store.sessions_with_queues().unwrap().is_empty());
}

/// Queuing a prompt is when the person sent it, so the hub sending it later isn't.
#[test]
fn a_queued_prompt_is_the_persons_as_its_queued_and_not_again_as_the_hub_sends_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    // Earlier than any clock could say, so a change can't land on the same moment.
    let forget = "UPDATE sessions SET user_active_at_ms = 1; UPDATE workspaces SET user_active_at_ms = 1";
    let forget = || lock(&store.writer).execute_batch(forget).unwrap();
    let workspace = || store.get_workspace(&"w".into()).unwrap().user_active_at_ms;
    forget();

    queue(&store, &queued("a", "first"));
    let queued_at = session(&store).user_active_at_ms;
    let workspace_at = workspace();
    forget();
    let sent = EventBody::UserPrompt { client_msg_id: Some("a".into()), prompt: Prompt::new("first") };
    let appended = store.append(&"s".into(), None, &sent, None, Some(&"a".into()), None).unwrap();

    assert!(queued_at.is_some_and(|at| at > 1), "{queued_at:?}");
    assert_eq!(workspace_at, queued_at, "its workspace moves with it");
    assert!(appended.dequeued && !appended.by_the_person);
    assert_eq!((session(&store).user_active_at_ms, workspace()), (Some(1), Some(1)));
}
