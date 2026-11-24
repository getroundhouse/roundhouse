//! What an open wait last read, which the hub keeps as an observation apart from the wait's record, and fills the wait's
//! watch with for the agent, and which the wait's record keeps, frozen, once it has ended.

use protocol::model::{Observation, Observed, PullRequestWatch};
use rusqlite::Connection;

use super::super::*;
use super::waiting::wait;
use crate::hub::tests::{hub_in, hub_in_again};
use crate::store::BEFORE_WAIT_READ_OBSERVED;
use crate::store::columns::OneValue;
use crate::store::older::store_from_before;
use crate::store::tests::idle_session;

/// What `hub` lists at `s`'s level of what its waits read.
async fn reads_listed(hub: &Hub) -> Vec<Observation> {
    let listed = hub.session_records("s".into()).await.unwrap().observations;
    listed.into_iter().filter(|observation| matches!(observation, Observation::WaitRead { .. })).collect()
}

/// The watch of the wait `a` as its record keeps it.
fn recorded(hub: &Hub) -> PullRequestWatch {
    *hub.store.wait(&"a".into()).unwrap().watch.unwrap()
}

/// A hub that starts again has what an open wait read as of its read, at the wait's session's level, and decides by it,
/// while the record keeps none of it.
#[tokio::test]
async fn what_an_open_wait_read_is_kept_across_a_restart_as_of_its_read() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.start_wait(wait()).await.unwrap();
    hub.observations_written().await;
    drop(hub);

    let again = hub_in_again(dir.path());

    let listed = reads_listed(&again).await;
    let [Observation::WaitRead { wait: id, read: Observed::AsOf { value, as_of_ms }, .. }] = listed.as_slice() else {
        panic!("not one read as of a time: {listed:?}")
    };
    let read = wait().watch.unwrap().read();
    assert_eq!((id.as_str(), value, *as_of_ms), ("a", &read, read.observed_at_ms));
    assert_eq!(recorded(&again).snapshot.pull_requests, [], "the record keeps none of it");
    let as_read = again.as_read(again.store.wait(&"a".into()).unwrap()).watch;
    assert_eq!(as_read, wait().watch, "what the hub decides by");
}

/// A wait that ends otherwise than by a judging, as the agent cancelling it, keeps in its record what it last read, and
/// the hub forgets it, as it does one of a wait a hub that starts finds has ended.
#[tokio::test]
async fn an_ended_waits_record_keeps_its_last_read_and_the_hub_forgets_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.start_wait(wait()).await.unwrap();

    hub.cancel_wait(&"s".into(), "a".into()).await.unwrap();

    assert_eq!(reads_listed(&hub).await, [], "forgotten as it ended");
    assert_eq!(recorded(&hub).read(), wait().watch.unwrap().read(), "the last read, frozen");
    let mut other = wait();
    other.id = "b".into();
    hub.start_wait(other).await.unwrap();
    hub.observations_written().await;
    drop(hub);
    let db = Connection::open(dir.path().join("hub.db")).unwrap();
    db.execute("UPDATE waits SET ended_at_ms = 5, ended = 'cancelled' WHERE id = 'b'", []).unwrap();
    let again = hub_in_again(dir.path());
    assert_eq!(reads_listed(&again).await, [], "one of a wait that ended while no hub kept it");
}

/// A database from before what a wait reads left its record opens with an open wait's record keeping none of what it
/// read, and nothing known of that, and an ended wait's as it ended. The open one is read again as it was asked for,
/// and no failure is taken for one.
#[tokio::test]
async fn a_wait_from_before_opens_with_what_it_read_not_known() {
    let dir = tempfile::tempdir().unwrap();
    let mut watch = *wait().watch.unwrap();
    watch.read_error = Some("offline".into());
    let open = serde_json::to_string(&watch).unwrap();
    let mode = serde_json::to_string(&idle_session(1).session.mode).unwrap();
    let rows = format!(
        "INSERT INTO sessions (id, workspace_id, agent, model, mode, effort, status, created_at_ms, updated_at_ms)
             VALUES ('s', 'w', 'claude', 'sonnet', '{mode}', 'high', 'idle', 0, 0);
         INSERT INTO waits (id, session_id, workspace_id, number, call, until, started_at_ms, watch)
             VALUES ('a', 's', 'w', 7, NULL, 'required_checks', 1, '{open}');
         INSERT INTO waits (id, session_id, workspace_id, number, call, until, started_at_ms, watch, ended_at_ms, ended)
             VALUES ('b', 's', 'w', 7, NULL, 'required_checks', 1, '{open}', 2, 'cancelled');"
    );
    drop(store_from_before(dir.path(), BEFORE_WAIT_READ_OBSERVED, &rows));

    let hub = hub_in_again(dir.path());

    let kept = recorded(&hub);
    assert_eq!((kept.snapshot.pull_requests.len(), kept.read_error), (0, None), "none of what it read");
    assert_eq!((&kept.requested, &kept.snapshot.repository), (&watch.requested, &watch.snapshot.repository));
    assert_eq!(*hub.store.wait(&"b".into()).unwrap().watch.unwrap(), watch, "an ended one as it ended");
    assert_eq!(reads_listed(&hub).await, [], "not known");
    let triggers: i64 = Connection::open(dir.path().join("hub.db"))
        .unwrap()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'trigger' AND name = 'waits_move_session_on_observation'",
            [],
            OneValue::read,
        )
        .unwrap();
    assert_eq!(triggers, 0);
}
