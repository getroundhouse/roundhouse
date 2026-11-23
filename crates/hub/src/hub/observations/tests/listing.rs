use std::sync::Arc;
use std::time::Duration;

use protocol::model::{Observation, ObservationKey, Observed, Rev};

use rusqlite::Connection;
use tokio::sync::oneshot;

use super::super::kinds::{Kind, WorkspaceChanges, WorkspaceLines};
use super::seen::{as_of, changes, lines, listed, of_w, told_copy};
use crate::hub::Hub;
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
/// it, and has the new copy, so no list's revision takes in a change the list lacks: the hub's, with the records, and a
/// workspace's. The change is held where it writes the cache file, by another writer holding the file.
#[tokio::test]
async fn a_list_asked_for_while_a_change_is_made_has_the_new_copy() {
    let of_the_hub = |hub: Arc<Hub>| async move {
        let listed = hub.records_list().await.unwrap();
        (listed.observations, listed.rev)
    };
    a_list_waits_for_a_change_made::<WorkspaceLines, _>([lines(3, 1), lines(4, 0)], of_the_hub).await;
    let of_w = |hub: Arc<Hub>| async move {
        let listed = hub.workspace_observations(&"w".into()).await.unwrap();
        (listed.observations, listed.rev)
    };
    a_list_waits_for_a_change_made::<WorkspaceChanges, _>([changes(3, 1), changes(4, 0)], of_w).await;
}

/// What [`a_list_asked_for_while_a_change_is_made_has_the_new_copy`] does for one kind, `K`, which it reads as
/// `values` in turn, and the list `list` reads, which has it.
async fn a_list_waits_for_a_change_made<K: Kind, F: Future<Output = (Vec<Observation>, Rev)> + Send + 'static>(
    [before, after]: [K::Value; 2],
    list: impl FnOnce(Arc<Hub>) -> F,
) where
    K::Of: From<&'static str>,
{
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_read::<K>("w".into(), before, 1_000).await.unwrap();
    let listed_before = hub.store.list_records().unwrap().rev;
    let mut feed = hub.subscribe_feed();
    let writer = Connection::open(dir.path().join("github-cache.db")).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let making = tokio::spawn({
        let (hub, after) = (hub.clone(), after.clone());
        async move { hub.observation_read::<K>("w".into(), after, 2_000).await }
    });
    let revised = || hub.store.list_records().unwrap().rev > listed_before;
    waiting::task::until(Duration::from_secs(5), "the change took no revision", revised).await;

    let mut asking = tokio::spawn(list(hub.clone()));

    assert!(tokio::time::timeout(HELD_FOR, &mut asking).await.is_err(), "the list waits for the change");
    writer.execute_batch("ROLLBACK;").unwrap();
    making.await.unwrap().unwrap();
    let (observations, rev) = asking.await.unwrap();
    let changed = told_copy(&mut feed);
    assert_eq!(observations, std::slice::from_ref(&changed), "the new copy");
    assert!(rev >= changed.rev());
    let read_then = Observed::AsOf { value: after, as_of_ms: 2_000 };
    assert_eq!(changed, K::observation(&"w".into(), read_then, changed.rev()));
}

/// Each observation's list, at its own level, sends only the observations of that level: the hub's has a workspace's
/// lines, and the workspace's has its changed files and nothing of another workspace.
#[tokio::test]
async fn each_level_is_listed_only_its_own_observations() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    for workspace in ["w", "v"] {
        hub.observation_read::<WorkspaceLines>(workspace.into(), lines(3, 1), 1_000).await.unwrap();
        hub.observation_read::<WorkspaceChanges>(workspace.into(), changes(3, 1), 1_000).await.unwrap();
    }

    let of_the_hub = hub.records_list().await.unwrap().observations;
    let of_w = hub.workspace_observations(&"w".into()).await.unwrap().observations;

    let mut keys: Vec<_> = of_the_hub.iter().map(Observation::key).collect();
    keys.sort_by_key(|key| format!("{key:?}"));
    let lines_of = |workspace: &str| ObservationKey::WorkspaceLines { workspace_id: workspace.into() };
    assert_eq!(keys, [lines_of("v"), lines_of("w")]);
    let [files] = of_w.try_into().unwrap();
    let read_then = Observed::AsOf { value: changes(3, 1), as_of_ms: 1_000 };
    assert_eq!(files, Observation::WorkspaceChanges { workspace_id: "w".into(), changes: read_then, rev: files.rev() });
}
