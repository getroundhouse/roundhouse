//! A chat's composer draft: kept, replaced and cleared, on a session that's there.

use protocol::model::{Draft, DraftToKeep};

use super::id;
use crate::store::StoreError;
use crate::store::tests::{open_in, store_with_session};

#[test]
fn a_draft_is_kept_until_another_replaces_it_and_an_empty_one_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert_eq!(store.get_session(&id()).unwrap().session.draft, Draft::default(), "nothing's typed yet");

    let kept = store.set_draft(&id(), &DraftToKeep::new("half a thought"), None).unwrap();

    assert!(kept.changed);
    assert_eq!(kept.session.session.draft.text, "half a thought");
    assert_eq!(kept.session.session, store.get_session(&id()).unwrap().session, "it comes back as stored");
    store.set_draft(&id(), &DraftToKeep::new("half a thought, and more"), None).unwrap();
    assert_eq!(store.get_session(&id()).unwrap().session.draft.text, "half a thought, and more");
    let cleared = store.set_draft(&id(), &DraftToKeep::default(), None).unwrap();
    assert!(cleared.changed);
    assert_eq!(store.get_session(&id()).unwrap().session.draft, Draft::default());
}

#[test]
fn a_change_to_the_draft_gives_the_session_a_newer_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_session(&id()).unwrap().session.rev;

    let after = store.set_draft(&id(), &DraftToKeep::new("typed"), None).unwrap().session.session.rev;

    assert!(after > before, "clients follow records by revision, so the feed would miss it otherwise");
}

#[test]
fn a_draft_for_a_session_that_isnt_there_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());

    assert!(matches!(
        store.set_draft(&id(), &DraftToKeep::new("typed"), None),
        Err(StoreError::NotFound { kind: "session", .. })
    ));
}
