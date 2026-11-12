//! Telling of each record's copies once, and in the order of their revisions, as two tasks that each read one back
//! after their own change can finish in either order.

use protocol::client::HubNotice;
use protocol::model::{NewDraft, RecordKey, Repo, Rev, Session, SessionId};
use tokio::sync::broadcast;

use super::Feed;
use crate::store::tests::store_with_session;

/// The session `s`'s record as it was, and as it is after a change, as two tasks read it back.
fn older_and_newer() -> (Session, Session) {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let id = SessionId::from("s");
    let older = store.get_session(&id).unwrap().session;
    let newer = store.set_draft(&id, &NewDraft::new("half a thought")).unwrap().session.session;
    assert!(newer.rev > older.rev, "the change gave the record a newer revision");
    (older, newer)
}

/// Everything a client following the feed has heard so far.
fn heard(client: &mut broadcast::Receiver<HubNotice>) -> Vec<HubNotice> {
    std::iter::from_fn(|| client.try_recv().ok()).collect()
}

fn told(session: &Session) -> HubNotice {
    HubNotice::Session(session.clone())
}

#[test]
fn a_copy_older_than_one_told_of_isnt_told_of() {
    let (older, newer) = older_and_newer();
    let feed = Feed::new();
    let mut client = feed.subscribe();
    feed.send(told(&newer));
    feed.send(told(&older));
    assert_eq!(heard(&mut client), [told(&newer)], "the older copy came too late");
}

#[test]
fn the_same_copy_is_told_of_once() {
    let (older, newer) = older_and_newer();
    let feed = Feed::new();
    let mut client = feed.subscribe();
    for session in [&older, &older, &newer, &newer] {
        feed.send(told(session));
    }
    assert_eq!(heard(&mut client), [told(&older), told(&newer)]);
}

#[test]
fn a_copy_older_than_its_records_removal_isnt_told_of() {
    let (_, newer) = older_and_newer();
    let feed = Feed::new();
    let mut client = feed.subscribe();
    let removed =
        HubNotice::Removed { record: RecordKey::Session { id: newer.id.clone() }, rev: Rev::new(newer.rev.get() + 1) };
    feed.send(removed.clone());
    feed.send(told(&newer));
    assert_eq!(heard(&mut client), [removed]);
}

#[test]
fn each_records_copies_are_in_order_apart_from_any_others() {
    let (older, newer) = older_and_newer();
    let other = Session { id: "t".into(), ..older.clone() };
    let repo = Repo { id: "s".into(), name: "s".into(), path: "/s".into(), created_at_ms: 0, rev: older.rev };
    let feed = Feed::new();
    let mut client = feed.subscribe();
    feed.send(told(&newer));
    feed.send(told(&other));
    feed.send(HubNotice::Repo(repo.clone()));
    assert_eq!(heard(&mut client), [told(&newer), told(&other), HubNotice::Repo(repo)]);
}

#[test]
fn a_copy_without_a_revision_and_a_resync_are_always_told_of() {
    let (older, _) = older_and_newer();
    let unrevised = Session { rev: Rev::default(), ..older };
    let feed = Feed::new();
    let mut client = feed.subscribe();
    feed.send(told(&unrevised));
    feed.send(told(&unrevised));
    feed.send(HubNotice::Resync);
    assert_eq!(heard(&mut client), [told(&unrevised), told(&unrevised), HubNotice::Resync]);
}
