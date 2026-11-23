use std::sync::Arc;
use std::time::Duration;

use protocol::client::HubNotice;
use protocol::model::{Observation, ObservationKey, Observed, Rev};

use rusqlite::Connection;
use tokio::sync::oneshot;

use super::super::kinds::{Kind, WorkspaceChanges, WorkspaceLines};
use super::seen::{as_of, changes, lines, listed, of_w, told, told_copy};
use crate::hub::Hub;
use crate::hub::tests::{hub_in, hub_in_again};

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

/// A list asked for while a change is being made, before it's kept, waits for it, and has the new copy, so no list's
/// revision takes in a change the list lacks: the hub's, with the records, and a workspace's. The change is held where
/// it takes its revision, by holding the database's writer.
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
#[expect(
    clippy::await_holding_lock,
    reason = "it holds the database's writer while the change waits for it, and nothing here writes the database"
)]
async fn a_list_waits_for_a_change_made<K: Kind, F: Future<Output = (Vec<Observation>, Rev)> + Send + 'static>(
    [before, after]: [K::Value; 2],
    list: impl FnOnce(Arc<Hub>) -> F,
) where
    K::Of: From<&'static str>,
{
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_read::<K>("w".into(), before, 1_000).await.unwrap();
    let mut feed = hub.subscribe_feed();
    let writes = hub.store.holding_writes();
    let making = tokio::spawn({
        let (hub, after) = (hub.clone(), after.clone());
        async move { hub.observation_read::<K>("w".into(), after, 2_000).await }
    });
    let changing = || hub.observations.changing.try_lock().is_err();
    waiting::task::until(Duration::from_secs(5), "the change didn't take the lock", changing).await;

    let mut asking = tokio::spawn(list(hub.clone()));

    assert!(tokio::time::timeout(HELD_FOR, &mut asking).await.is_err(), "the list waits for the change");
    drop(writes);
    making.await.unwrap().unwrap();
    let (observations, rev) = asking.await.unwrap();
    let changed = told_copy(&mut feed);
    assert_eq!(observations, std::slice::from_ref(&changed), "the new copy");
    assert!(rev >= changed.rev());
    let read_then = Observed::AsOf { value: after, as_of_ms: 2_000 };
    assert_eq!(changed, K::observation(&"w".into(), read_then, changed.rev()));
}

/// A change made while another process holds the cache file, which can hold up its write for seconds, is kept and told
/// at once, and a list asked for then has it, since nothing waits on the file under the lock lists take. Its writes
/// land once the file is let go, in the order the changes were made, and dropping the hub waits for them, so a hub
/// that starts again has the last.
#[tokio::test]
async fn a_list_waits_for_no_write_to_the_cache_file() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut feed = hub.subscribe_feed();
    let writer = Connection::open(dir.path().join("github-cache.db")).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE;").unwrap();

    let changing = async {
        hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.unwrap();
        hub.observation_read::<WorkspaceLines>("w".into(), lines(4, 0), 2_000).await.unwrap();
        listed(&hub).await
    };
    // Well inside the 5 s a write waits for the file, so a change that waited for its write would fail this.
    let quickly = Duration::from_secs(2);
    let (observations, _) = tokio::time::timeout(quickly, changing).await.expect("nothing waits on the file");

    let changed = told(&mut feed);
    let [.., HubNotice::Observation(last)] = changed.as_slice() else { panic!("told of no change: {changed:?}") };
    assert_eq!(observations, std::slice::from_ref(last), "the new copy");
    let mut dropping = tokio::task::spawn_blocking(move || drop(hub));
    assert!(tokio::time::timeout(HELD_FOR, &mut dropping).await.is_err(), "dropping the hub waits for its writes");
    writer.execute_batch("ROLLBACK;").unwrap();
    dropping.await.unwrap();
    let (observations, _) = listed(&hub_in_again(dir.path())).await;
    let [kept] = observations.try_into().unwrap();
    assert_eq!(kept, of_w(as_of(lines(4, 0), 2_000), kept.rev()), "the last write lands last");
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

/// A level's list has the observations of that level alone, under the revision the hub was at as it listed them, and
/// the hub's level keeps none of another.
#[tokio::test]
async fn a_level_s_list_has_its_own_observations_alone() {
    use protocol::model::{
        GithubRepositoryName, IssueNumber, Level, Observation, Observed, ReferenceAsk, ReferenceSaid,
    };

    use super::super::kinds::Referenced;

    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let ask =
        |n| ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(n).unwrap() };
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.unwrap();
    hub.observation_worked_out::<Referenced>(ask(1), ReferenceSaid::Missing, 1_000).await.unwrap();
    hub.observation_worked_out::<Referenced>(ask(2), ReferenceSaid::Missing, 2_000).await.unwrap();
    let rev_now = async { Ok(hub.store.current_rev().unwrap()) };

    let (listed_at, rev) = hub.observations.listed_at(&Level::Reference { reference: ask(1) }, rev_now).await.unwrap();

    let missing = Observed::AsOf { value: ReferenceSaid::Missing, as_of_ms: 1_000 };
    let [one] = listed_at.as_slice() else { panic!("{listed_at:?}") };
    assert_eq!(*one, Observation::Reference { reference: ask(1), said: missing, rev: one.rev() });
    assert!(rev > one.rev(), "{rev} covers the second, made after");
    assert_eq!(listed(&hub).await.0.len(), 1, "the hub's level lists only the lines");
}
