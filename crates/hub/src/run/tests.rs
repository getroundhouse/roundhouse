use std::time::Duration;

use protocol::model::{LineCounts, Observation, Observed, WorkspaceId, WorkspaceStatus};
use rusqlite::Connection;

use super::*;
use crate::hub::WorkspaceLines;
use crate::hub::tests::{hub_in, hub_in_again};
use crate::store::tests::store_with_session;
use crate::store::{InstanceStage, Move};

/// A workspace the last run left half-made is discarded as the hub starts, and its instance is handed over to be
/// destroyed once the hub is serving, whether it was still being made or its provider had made it.
#[test]
fn a_half_made_workspace_is_discarded_and_its_instance_handed_over_to_be_destroyed() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut half_made = store.get_workspace(&"w".into()).unwrap();
    half_made.status = WorkspaceStatus::Creating;
    for (id, instance_id) in [("birch", "birch-vm"), ("cedar", "cedar-vm")] {
        (half_made.id, half_made.name, half_made.instance_id) = (id.into(), id.into(), instance_id.into());
        half_made.created_at_ms += 1;
        store.insert_workspace(&half_made, &crate::store::WorkspacePlacement::local_checkout("/repo"), None).unwrap();
    }
    store.instance_made(&"cedar-vm".into()).unwrap();

    let unfinished = recover(&store).unwrap();

    let handed_over: Vec<_> =
        unfinished.instances.iter().map(|instance| (instance.id.to_string(), instance.stage)).collect();
    let destroying = |id: &str| (id.to_string(), InstanceStage::Destroying);
    assert_eq!(handed_over, [destroying("birch-vm"), destroying("cedar-vm")]);
    assert!(store.get_workspace(&"birch".into()).is_err(), "the workspace is discarded");
    assert!(store.get_workspace(&"cedar".into()).is_err(), "the workspace is discarded");
    assert!(store.get_workspace(&"w".into()).is_ok(), "a ready one stays");
}

/// How far archiving or restoring got is the worker's to say, so each workspace's record goes on saying what the last
/// run was doing, and the hub is handed them to settle once it's serving.
#[test]
fn a_workspace_left_being_archived_or_restored_is_handed_over_as_it_was_left() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut restoring = store.get_workspace(&"w".into()).unwrap();
    (restoring.id, restoring.name, restoring.instance_id) = ("birch".into(), "birch".into(), "birch-vm".into());
    restoring.status = WorkspaceStatus::Restoring;
    store.insert_workspace(&restoring, &crate::store::WorkspacePlacement::local_checkout("/repo"), None).unwrap();
    store.start_move(&"w".into(), Move::Archive).unwrap().unwrap();

    let left = recover(&store).unwrap();

    let (ready, archived) = (WorkspaceId::from("w"), WorkspaceId::from("birch"));
    assert_eq!(left.moves, [(ready, Move::Archive), (archived, Move::Restore)]);
    assert_eq!(store.get_workspace(&"w".into()).unwrap().status, WorkspaceStatus::Archiving);
    assert_eq!(store.get_workspace(&"birch".into()).unwrap().status, WorkspaceStatus::Restoring);
}

/// A setup the last run left running never finished, and its worker can't tell this hub how it ended, so it's recorded
/// as failed rather than as running for ever.
#[test]
fn a_starting_hub_records_a_setup_the_last_left_running_as_failed() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_workspace_setup(&"w".into(), protocol::model::SetupStatus::Running).unwrap();

    recover(&store).unwrap();

    let setup = store.get_workspace(&"w".into()).unwrap().setup;
    assert_eq!(setup, protocol::model::SetupStatus::Failed { code: None }, "it's left as the last run left it");
}

/// What GitHub is asked after a sign-in with was only in the last hub's memory, so none it left waiting can go on.
#[test]
fn a_starting_hub_records_every_sign_in_with_github_the_last_left_waiting_as_ended() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let waiting = protocol::fixtures::github_sign_in("s").status;
    store.start_github_sign_in(&"s".into(), &waiting).unwrap();

    recover(&store).unwrap();

    let statuses: Vec<_> =
        store.list_records().unwrap().github_sign_ins.into_iter().map(|sign_in| sign_in.status).collect();
    assert_eq!(statuses, [protocol::model::GithubSignInStatus::Ended]);
}

/// What a hub started can still be running once it lets go of its lock, which the next hub can take at once, so its
/// store stops writing first.
#[test]
fn letting_go_of_the_lock_stops_the_store_writing() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(store_with_session(dir.path()));
    let path = dir.path().join("hub.lock");
    let file = std::fs::File::create(&path).unwrap();
    file.try_lock().unwrap();
    let lock = HubLock { store: store.clone(), _lock: file };
    assert!(std::fs::File::open(&path).unwrap().try_lock().is_err(), "it's held");

    drop(lock);

    assert!(store.writer_is_held());
    // Once a process another test starts has run its program, since until then it has a copy of the lock's file, and
    // the lock goes with the last copy.
    waiting::thread::until_ok(Duration::from_secs(5), || {
        std::fs::File::open(&path).unwrap().try_lock().map_err(|e| format!("the lock is still held: {e}"))
    });
}

/// A stopping hub lets go of its lock only once every observation it changed is written to the cache file, which
/// another process can hold up for seconds, since the next hub can take the lock at once, and starts with what the file
/// kept.
#[tokio::test]
async fn a_stopping_hub_lets_go_of_its_lock_once_its_observations_are_written() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let path = dir.path().join("hub.lock");
    let file = std::fs::File::create(&path).unwrap();
    file.try_lock().unwrap();
    // A store of its own, which letting go stops writing, so the hub's own is there for the next.
    let elsewhere = tempfile::tempdir().unwrap();
    let lock = HubLock { store: Arc::new(store_with_session(elsewhere.path())), _lock: file };
    let holder = Connection::open(dir.path().join("github-cache.db")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let lines = LineCounts { added: 3, removed: 1 };
    hub.observation_read::<WorkspaceLines>("w".into(), lines, 1_000).await.unwrap();

    let mut letting_go = tokio::spawn({
        let hub = hub.clone();
        async move { let_go(&hub, lock).await }
    });

    // Well inside the 5 s a write waits for the file, after which it's given up.
    let held_for = Duration::from_millis(200);
    assert!(tokio::time::timeout(held_for, &mut letting_go).await.is_err(), "it let go before the write");
    assert!(std::fs::File::open(&path).unwrap().try_lock().is_err(), "the lock went before the write");
    holder.execute_batch("ROLLBACK;").unwrap();
    letting_go.await.unwrap();
    // Once a process another test starts has run its program, since until then it has a copy of the lock's file.
    let free = || std::fs::File::open(&path).unwrap().try_lock().is_ok();
    waiting::task::until(Duration::from_secs(5), "the lock to be let go", free).await;
    drop(hub);
    let records = hub_in_again(dir.path()).records_list().await.unwrap();
    let [Observation::WorkspaceLines { lines: kept, .. }] = records.observations.as_slice() else {
        panic!("not the lines alone: {:?}", records.observations);
    };
    assert_eq!(*kept, Observed::AsOf { value: lines, as_of_ms: 1_000 }, "the next hub has the write");
}
