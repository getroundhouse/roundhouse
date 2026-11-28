use std::sync::Arc;
use std::time::Duration;

use dialing::{Connection, Relays, WorkerEndpoint};
use protocol::client::{ErrorCode, Outcome};
use protocol::dialing::{Admission, Admitted, Ticket};
use protocol::model::{InstanceId, ProviderName};
use tokio::io::{AsyncBufReadExt, BufReader};

use super::*;
use crate::store::tests::store_with_session;

const WITHIN: Duration = Duration::from_secs(10);

async fn listening(dir: &std::path::Path) -> WorkerDial {
    let store = Arc::new(store_with_session(dir));
    let dial = WorkerDial::new(dir, Relays::None, EndpointPorts::PRODUCT, store);
    dial.listen(Endpoint::Loopback).await.unwrap();
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
    let ticket = dial.ticket(&"w".into(), Endpoint::Loopback).await.unwrap().read().unwrap();
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
    let older = dial.ticket(&"w".into(), Endpoint::Loopback).await.unwrap().read().unwrap();
    let newer = dial.ticket(&"w".into(), Endpoint::Loopback).await.unwrap().read().unwrap();
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

    let ticket = dial.ticket(&"elsewhere".into(), Endpoint::Loopback).await;

    assert_eq!(ticket.unwrap_err().code, ErrorCode::NotFound);
}

/// So a hub whose workers all run elsewhere binds no endpoint as it starts, and one whose workers all run on its own
/// machine binds only the loopback one, and reaches no relay.
#[tokio::test]
async fn a_hub_knows_which_providers_workers_dial_it_from_its_instances_records() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(store_with_session(dir.path()));
    let dial = WorkerDial::new(dir.path(), Relays::None, EndpointPorts::PRODUCT, store.clone());

    let before = store.providers_with_dialing_workers().unwrap();
    dial.ticket(&"w".into(), Endpoint::Loopback).await.unwrap();

    assert!(before.is_empty());
    assert_eq!(store.providers_with_dialing_workers().unwrap(), [ProviderName::from(ProviderName::LOCAL)]);
    assert!(store.instance_dials(&"w".into()).unwrap());
}

/// The owner's: a hub whose workers all run on its own machine reaches nothing beyond loopback, with no relay, no
/// port mapping and no address lookup, even where its endpoint for other machines' workers would use n0's relays.
#[tokio::test]
async fn a_ticket_for_a_worker_on_the_hubs_machine_names_loopback_alone_and_no_relay() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(store_with_session(dir.path()));
    let dial = WorkerDial::new(dir.path(), Relays::N0, EndpointPorts::PRODUCT, store);

    let ticket = dial.ticket(&"w".into(), Endpoint::Loopback).await.unwrap().read().unwrap();

    assert!(ticket.hub.relay.is_none(), "{:?}", ticket.hub);
    assert!(!ticket.hub.direct.is_empty(), "{:?}", ticket.hub);
    assert!(ticket.hub.direct.iter().all(|addr| addr.ip().is_loopback()), "{:?}", ticket.hub);
    assert!(dial.is_bound(Endpoint::Loopback));
    assert!(!dial.is_bound(Endpoint::Relayed), "nothing but loopback was bound");
    let written = dialing::address_file::read(&dir.path().join(protocol::paths::LOOPBACK_WORKER_ADDRESS_FILE));
    assert_eq!(written.unwrap(), ticket.hub, "where a local worker finds it once a dial has failed");
}

/// Both endpoints admit by the same records, so a worker is admitted wherever it dials, by its ticket and then its key.
#[tokio::test]
async fn either_endpoint_admits_a_worker_by_a_ticket_the_other_made() {
    let dir = tempfile::tempdir().unwrap();
    let dial = listening(dir.path()).await;
    let ticket = dial.ticket(&"w".into(), Endpoint::Loopback).await.unwrap().read().unwrap();
    let relayed = dial.listen(Endpoint::Relayed).await.unwrap().address().await;
    let worker = WorkerEndpoint::bind(&relayed, None, None).await.unwrap();

    let (_connection, outcome) = dial_as(&worker, &Ticket { hub: relayed, ..ticket }).await;

    assert_eq!(admitted_as(outcome), InstanceId::from("w"));
    assert!(dial.worker(&"w".into(), WITHIN).await.is_some());
}

/// A test's hub given ports that would run past the last is refused them, rather than given fewer.
#[test]
fn a_tests_ports_past_the_last_port_are_none() {
    assert!(EndpointPorts::for_a_test(u16::MAX - 2).is_none());
    let ports = EndpointPorts::for_a_test(10_000).unwrap();
    assert!(ports.loopback.contains(10_001) && !ports.loopback.contains(10_002));
    assert!(ports.relayed.contains(10_002) && ports.relayed.contains(10_003));
}
