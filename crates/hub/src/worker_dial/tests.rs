use std::sync::Arc;
use std::time::Duration;

use dialing::{Connection, Relays, WorkerEndpoint};
use protocol::client::{ErrorCode, Outcome};
use protocol::dialing::{Admission, Admitted, Ticket};
use protocol::model::InstanceId;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::*;
use crate::store::tests::store_with_session;

const WITHIN: Duration = Duration::from_secs(10);

async fn listening(dir: &std::path::Path) -> WorkerDial {
    let store = Arc::new(store_with_session(dir));
    let dial = WorkerDial::new(dir.join("worker-endpoint.json"), Relays::None, store);
    dial.listen().await.unwrap();
    dial
}

/// Dials as a worker given `ticket` would, with its own key, and says what admits it.
async fn dial_as(worker: &WorkerEndpoint, ticket: &Ticket) -> (Connection, Outcome<Admitted>) {
    let connection = worker.dial().await.unwrap();
    let (mut send, recv) = connection.open_bi().await.unwrap();
    let admission = Admission { secret: ticket.secret.clone() };
    send.write_all(&protocol::frame::encode(&admission).unwrap()).await.unwrap();
    let mut line = String::new();
    BufReader::new(recv).read_line(&mut line).await.unwrap();
    (connection, protocol::frame::decode(line.as_bytes()).unwrap())
}

fn admitted_as(outcome: Outcome<Admitted>) -> InstanceId {
    let Outcome::Response(Admitted { instance_id }) = outcome else { panic!("refused: {outcome:?}") };
    instance_id
}

/// So a ticket admits one worker, once, and that worker again whenever it dials, as after a restart of the hub.
#[tokio::test]
async fn a_ticket_admits_one_worker_once_and_that_worker_again_by_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let dial = listening(dir.path()).await;
    let ticket = dial.ticket(&"w".into()).await.unwrap().read().unwrap();
    let worker = WorkerEndpoint::bind(&ticket.hub, None, None).await.unwrap();
    let another = WorkerEndpoint::bind(&ticket.hub, None, None).await.unwrap();

    let (_first, first) = dial_as(&worker, &ticket).await;
    let arrived = dial.worker(&"w".into(), WITHIN).await.unwrap();
    let (_refused, reused) = dial_as(&another, &ticket).await;
    let (_again, again) = dial_as(&worker, &ticket).await;

    assert_eq!(admitted_as(first), InstanceId::from("w"));
    assert_eq!(arrived.instance_id, InstanceId::from("w"));
    let Outcome::Error(refusal) = reused else { panic!("a used ticket admitted another worker") };
    assert_eq!(refusal.code, ErrorCode::Unauthorized);
    assert_eq!(admitted_as(again), InstanceId::from("w"), "known by its key");
    assert!(dial.worker(&"w".into(), WITHIN).await.is_some(), "and its connection is the instance's from then on");
}

/// Only the newest ticket of an instance's admits a worker, so one handed to a provider that didn't use it is no use
/// to anyone else.
#[tokio::test]
async fn a_newer_ticket_for_an_instance_leaves_the_one_before_admitting_none() {
    let dir = tempfile::tempdir().unwrap();
    let dial = listening(dir.path()).await;
    let older = dial.ticket(&"w".into()).await.unwrap().read().unwrap();
    let newer = dial.ticket(&"w".into()).await.unwrap().read().unwrap();
    let worker = WorkerEndpoint::bind(&older.hub, None, None).await.unwrap();

    let (_, with_older) = dial_as(&worker, &older).await;
    let (_, with_newer) = dial_as(&worker, &newer).await;

    assert!(matches!(with_older, Outcome::Error(_)), "{with_older:?}");
    assert_eq!(admitted_as(with_newer), InstanceId::from("w"));
}

#[tokio::test]
async fn an_instance_the_hub_has_no_record_of_gets_no_ticket() {
    let dir = tempfile::tempdir().unwrap();
    let dial = listening(dir.path()).await;

    let ticket = dial.ticket(&"elsewhere".into()).await;

    assert_eq!(ticket.unwrap_err().code, ErrorCode::NotFound);
}

/// So a hub whose workers all run on its own machine binds no endpoint as it starts, and reaches no relay.
#[tokio::test]
async fn a_hub_knows_whether_any_worker_dials_it_from_its_instances_records() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(store_with_session(dir.path()));
    let dial = WorkerDial::new(dir.path().join("worker-endpoint.json"), Relays::None, store.clone());

    let before = store.has_dialing_workers().unwrap();
    dial.ticket(&"w".into()).await.unwrap();

    assert!(!before);
    assert!(store.has_dialing_workers().unwrap());
}
