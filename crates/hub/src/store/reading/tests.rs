use protocol::model::{Rev, SessionStatus};

use super::*;
use crate::store::StatusChange;
use crate::store::tests::{notice, store_with_session};

const TURN_ENDS: StatusChange = StatusChange { status: SessionStatus::Idle, ends_turn: true, background_tasks: None };

/// A prompt, then the event that ends its turn. Returns that event's `seq`.
fn turn(store: &Store) -> Seq {
    store.append(&"s".into(), None, &notice("prompt"), None, None, Some(SessionStatus::Running.into())).unwrap();
    store.append(&"s".into(), None, &notice("done"), None, None, Some(TURN_ENDS)).unwrap().event.seq
}

fn session(store: &Store) -> protocol::model::Session {
    store.get_session(&"s".into()).unwrap().session
}

#[test]
fn the_end_of_a_turn_is_an_unread_reply_until_its_marked_read() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert!(!session(&store).unread(), "a new session has no reply");

    let reply = turn(&store);

    assert_eq!(session(&store).reply_seq, reply);
    assert!(session(&store).unread());
    let read = store.mark_read(&"s".into(), reply).unwrap();
    assert!(read.changed);
    assert!(!read.session.session.unread());
    assert_eq!(read.session.session, session(&store), "it comes back as stored");
}

#[test]
fn a_change_of_status_that_doesnt_end_a_turn_leaves_the_reply_alone() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let reply = turn(&store);

    store.append(&"s".into(), None, &notice("again"), None, None, Some(SessionStatus::Running.into())).unwrap();
    store.append(&"s".into(), None, &notice("gone"), None, None, Some(SessionStatus::Interrupted.into())).unwrap();

    assert_eq!(session(&store).reply_seq, reply);
}

#[test]
fn marking_read_never_reaches_a_reply_still_to_come() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    turn(&store);

    store.mark_read(&"s".into(), Seq::MAX).unwrap();
    let next = turn(&store);

    assert_eq!(session(&store).reply_seq, next);
    assert!(session(&store).unread());
}

#[test]
fn marking_read_only_moves_forward() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let first = turn(&store);
    let second = turn(&store);
    store.mark_read(&"s".into(), second).unwrap();

    let earlier = store.mark_read(&"s".into(), first).unwrap();

    assert!(!earlier.changed);
    assert_eq!(session(&store).read_seq, second);
}

#[test]
fn marking_unread_brings_back_only_the_newest_reply() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert!(!store.mark_unread(&"s".into()).unwrap().changed, "there's no reply to mark");
    turn(&store);
    let newest = turn(&store);
    store.mark_read(&"s".into(), newest).unwrap();

    let unread = store.mark_unread(&"s".into()).unwrap();

    assert!(unread.changed);
    assert!(unread.session.session.unread());
    assert_eq!(unread.session.session.read_seq, newest - 1);
    assert!(!store.mark_unread(&"s".into()).unwrap().changed, "it's unread already");
    assert!(store.mark_read(&"s".into(), newest).unwrap().changed, "and it can be read again");
}

/// A client only takes a copy with a newer revision, so each change has to move it, and asking for what's already
/// so mustn't, since nothing would announce it.
#[test]
fn only_a_change_to_how_far_its_read_moves_its_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let reply = turn(&store);
    let rev = |store: &Store| session(store).rev;
    let replied = rev(&store);

    let read = store.mark_read(&"s".into(), reply).unwrap().session.session.rev;
    let read_again = store.mark_read(&"s".into(), reply).unwrap().session.session.rev;
    let unread = store.mark_unread(&"s".into()).unwrap().session.session.rev;
    let unread_again = store.mark_unread(&"s".into()).unwrap().session.session.rev;

    assert!(replied > Rev::new(1) && read > replied && unread > read);
    assert_eq!((read_again, unread_again), (read, unread));
    assert_eq!(store.list_records().unwrap().rev, unread);
}

#[test]
fn marking_a_session_that_doesnt_exist_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let missing = store.mark_read(&"nope".into(), 1).map(|_| ()).unwrap_err();

    assert!(matches!(missing, StoreError::NotFound { kind: "session", .. }));
}
