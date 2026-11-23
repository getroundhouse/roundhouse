use std::time::Duration;

use rusqlite::Connection;
use tokio::sync::oneshot;

use super::super::kinds::WorkspaceLines;
use super::seen::{as_of, lines, listed, of_w, told_copy};
use crate::hub::tests::hub_in;

/// How long a test gives a change, or a list, to finish while what it waits for is held, which it mustn't.
const HELD_FOR: Duration = Duration::from_millis(200);

/// A change asked for while the records are being listed waits for the list, which has the copy from before it, and
/// clients are told of the change after, under a newer revision than the list's, so a client keeps the new copy.
#[tokio::test]
async fn a_change_made_while_the_records_are_listed_is_told_after_the_list() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.unwrap();
    let (before, _) = listed(&hub).await;
    let mut feed = hub.subscribe_feed();
    let ((listing, began), (go, gone_on)) = (oneshot::channel(), oneshot::channel::<()>());
    let asking = tokio::spawn({
        let hub = hub.clone();
        async move {
            let list = async {
                listing.send(()).unwrap();
                gone_on.await.unwrap();
                hub.store.list_records().unwrap().rev
            };
            hub.observations.listed_with(list).await
        }
    });
    began.await.unwrap();

    let mut making = tokio::spawn({
        let hub = hub.clone();
        async move { hub.observation_read::<WorkspaceLines>("w".into(), lines(4, 0), 2_000).await }
    });

    assert!(tokio::time::timeout(HELD_FOR, &mut making).await.is_err(), "the change waits for the list");
    go.send(()).unwrap();
    let (rev, observations) = asking.await.unwrap();
    making.await.unwrap().unwrap();
    assert_eq!(observations, before, "the copy from before");
    let changed = told_copy(&mut feed);
    assert!(changed.rev() > rev, "{changed:?} after a list at {rev}");
    assert_eq!(changed, of_w(as_of(lines(4, 0), 2_000), changed.rev()));
}

/// A list asked for while a change is being made, once the change has its revision and before it's kept, waits for
/// it, and has the new copy, so no list's revision takes in a change the list lacks. The change is held where it
/// writes the cache file, by another writer holding the file.
#[tokio::test]
async fn a_list_asked_for_while_a_change_is_made_has_the_new_copy() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.unwrap();
    let (_, before) = listed(&hub).await;
    let mut feed = hub.subscribe_feed();
    let writer = Connection::open(dir.path().join("github-cache.db")).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let making = tokio::spawn({
        let hub = hub.clone();
        async move { hub.observation_read::<WorkspaceLines>("w".into(), lines(4, 0), 2_000).await }
    });
    let revised = || hub.store.list_records().unwrap().rev > before;
    waiting::task::until(Duration::from_secs(5), "the change took no revision", revised).await;

    let mut asking = tokio::spawn({
        let hub = hub.clone();
        async move { hub.records_list().await.unwrap() }
    });

    assert!(tokio::time::timeout(HELD_FOR, &mut asking).await.is_err(), "the list waits for the change");
    writer.execute_batch("ROLLBACK;").unwrap();
    making.await.unwrap().unwrap();
    let listed = asking.await.unwrap();
    let changed = told_copy(&mut feed);
    assert_eq!(listed.observations, std::slice::from_ref(&changed), "the new copy");
    assert!(listed.rev >= changed.rev());
    assert_eq!(changed, of_w(as_of(lines(4, 0), 2_000), changed.rev()));
}
