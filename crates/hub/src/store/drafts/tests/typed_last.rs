//! Of two changes to a chat's composer draft, the one typed last is kept, by when each says it was typed, as
//! `SessionDraft` says.

use protocol::model::{Draft, DraftToKeep};

use super::id;
use crate::store::tests::store_with_session;
use crate::store::{Store, StoreError, lock, now_ms};

/// What the session's composer has now.
fn kept(store: &Store) -> Draft {
    store.get_session(&id()).unwrap().session.draft
}

#[test]
fn a_change_typed_later_replaces_the_draft_and_one_typed_earlier_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_draft(&id(), &DraftToKeep::new("typed at the desk"), Some(2_000)).unwrap();

    let offline = store.set_draft(&id(), &DraftToKeep::new("typed on the train"), Some(1_000));
    let later = store.set_draft(&id(), &DraftToKeep::new("typed after"), Some(3_000)).unwrap();

    assert!(matches!(offline, Err(StoreError::TypedEarlier)), "it was kept");
    assert!(later.changed);
    assert_eq!(kept(&store).text, "typed after");
}

#[test]
fn a_change_typed_at_the_same_moment_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_draft(&id(), &DraftToKeep::new("first"), Some(2_000)).unwrap();

    let same = store.set_draft(&id(), &DraftToKeep::new("second"), Some(2_000)).unwrap();

    assert!(same.changed, "one of two typed together has to win, so it's the one that came last, as before");
    assert_eq!(kept(&store).text, "second");
}

#[test]
fn an_emptied_draft_keeps_when_it_was_emptied() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_draft(&id(), &DraftToKeep::new("half a thought"), Some(1_000)).unwrap();
    store.set_draft(&id(), &DraftToKeep::default(), Some(3_000)).unwrap();

    let offline = store.set_draft(&id(), &DraftToKeep::new("typed before it was sent"), Some(2_000));

    assert!(matches!(offline, Err(StoreError::TypedEarlier)), "it was kept");
    assert_eq!(kept(&store), Draft::default(), "what was sent doesn't come back");
}

#[test]
fn what_the_draft_says_already_is_never_refused_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.set_draft(&id(), &DraftToKeep::new("typed"), Some(2_000)).unwrap().session.session.rev;

    let again = store.set_draft(&id(), &DraftToKeep::new("typed"), Some(1_000)).unwrap();

    assert!(!again.changed);
    assert_eq!(again.session.session.rev, before);
}

#[test]
fn the_same_draft_typed_later_moves_when_it_was_typed_and_no_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.set_draft(&id(), &DraftToKeep::new("typed"), Some(1_000)).unwrap().session.session.rev;

    let again = store.set_draft(&id(), &DraftToKeep::new("typed"), Some(3_000)).unwrap();
    let between = store.set_draft(&id(), &DraftToKeep::new("typed between"), Some(2_000));

    assert!(!again.changed);
    assert_eq!(again.session.session.rev, before, "no record shows when it was typed");
    assert!(matches!(between, Err(StoreError::TypedEarlier)), "it was kept");
}

#[test]
fn a_change_from_a_client_that_doesnt_say_when_it_was_typed_is_typed_as_it_comes() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let typed = now_ms();
    store.set_draft(&id(), &DraftToKeep::new("typed here"), Some(typed)).unwrap();

    let older_client = store.set_draft(&id(), &DraftToKeep::new("typed on an older build"), None).unwrap();
    let before_it = store.set_draft(&id(), &DraftToKeep::new("typed before"), Some(typed - 1));

    assert!(older_client.changed);
    assert!(matches!(before_it, Err(StoreError::TypedEarlier)), "it was kept");
}

#[test]
fn a_change_from_a_clock_that_runs_ahead_counts_as_typed_now() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_draft(&id(), &DraftToKeep::new("typed on a fast clock"), Some(i64::MAX)).unwrap();

    let after = store.set_draft(&id(), &DraftToKeep::new("typed after it"), Some(now_ms() + 1_000)).unwrap();

    assert!(after.changed, "otherwise no change could replace it until the clocks met");
    assert_eq!(kept(&store).text, "typed after it");
}

#[test]
fn a_draft_kept_before_its_time_was_is_replaced_by_any_change() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_draft(&id(), &DraftToKeep::new("kept by an older hub"), None).unwrap();
    lock(&store.writer).execute("UPDATE sessions SET draft_typed_at_ms = NULL", []).unwrap();

    let typed_long_ago = store.set_draft(&id(), &DraftToKeep::new("typed long ago"), Some(1)).unwrap();

    assert!(typed_long_ago.changed);
}
