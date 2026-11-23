//! A worker that dials its hub with a ticket, as a provider other than the local one starts it, against a stand-in
//! for the hub's endpoint that admits it by hand.

use std::path::Path;
use std::process::{Child, Stdio};

use dialing::{HubEndpoint, Relays, read_frame, remote_key, write_frame};
use process::testing::WholeEnvironment;
use protocol::client::Outcome;
use protocol::dialing::{Admission, Admitted, Ticket, TicketSecret, WorkerTicket};
use protocol::worker::{
    Hello, WORKER_PROTOCOL_VERSION, WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse,
};

use crate::support::TIMEOUT;

fn start_worker(dir: &Path, ticket: &WorkerTicket) -> Child {
    WholeEnvironment::for_a_test(&dir.join("home"))
        .command(crate::support::daemon())
        .args(["worker", "--ticket", ticket.as_str(), "--log"])
        .arg(dir.join("worker.log"))
        .arg("--workspaces-root")
        .arg(dir.join("workspaces"))
        .arg("--state-dir")
        .arg(dir.join("instance"))
        .arg("--clones-dir")
        .arg(dir.join("clones"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .unwrap()
}

/// The next worker that dials, admitted as instance `i` whatever it says, and what it said.
async fn admit(
    hub: &HubEndpoint,
) -> (dialing::Connection, dialing::SendStream, dialing::RecvStream, String, Admission) {
    let connection = hub.accept().await.unwrap().await.unwrap();
    let (mut send, mut recv) = connection.accept_bi().await.unwrap();
    let admission: Admission = read_frame(&mut recv).await.unwrap();
    write_frame(&mut send, &Outcome::Response(Admitted { instance_id: "i".into() })).await.unwrap();
    let key = remote_key(&connection);
    (connection, send, recv, key, admission)
}

async fn call(
    send: &mut dialing::SendStream,
    recv: &mut dialing::RecvStream,
    id: u64,
    request: WorkerRequest,
) -> WorkerOutcome {
    write_frame(send, &WorkerCall { id, request }).await.unwrap();
    loop {
        match read_frame::<WorkerFrame>(recv).await.unwrap() {
            WorkerFrame::Reply { id: answering, outcome } if answering == id => return outcome,
            _ => {}
        }
    }
}

fn hello() -> WorkerRequest {
    WorkerRequest::Hello(Hello {
        protocol_version: WORKER_PROTOCOL_VERSION,
        oldest_protocol_version: None,
        keep_running_without_hub: true,
    })
}

/// So a worker whose connection drops, as when its hub restarts, carries on and dials again with the same key, which
/// the hub admits it by, and stops once a hub asks it to.
#[test]
fn a_worker_given_a_ticket_dials_its_hub_and_dials_again_once_the_connection_drops() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let hub = HubEndpoint::bind(&dir.path().join("endpoint.json"), Relays::None).await.unwrap();
        let secret = TicketSecret::from("the-secret".to_string());
        let ticket = WorkerTicket::from(&Ticket { hub: hub.address().await, secret: secret.clone() });
        let mut worker = start_worker(dir.path(), &ticket);

        let (first, mut send, mut recv, key, said) = tokio::time::timeout(TIMEOUT, admit(&hub)).await.unwrap();
        let greeted = call(&mut send, &mut recv, 1, hello()).await;
        first.close(0u32.into(), b"the hub went");
        let (_again, mut send, mut recv, key_again, said_again) =
            tokio::time::timeout(TIMEOUT, admit(&hub)).await.unwrap();
        let greeted_again = call(&mut send, &mut recv, 1, hello()).await;
        let stopped = call(&mut send, &mut recv, 2, WorkerRequest::Stop).await;
        // As a hub does once it has asked a worker to stop.
        send.finish().unwrap();
        let exited =
            waiting::thread::until_ok(TIMEOUT, || worker.try_wait().unwrap().ok_or("the worker is still running"));

        assert_eq!((said.secret, said_again.secret), (secret.clone(), secret));
        assert!(matches!(greeted, WorkerOutcome::Response(WorkerResponse::Hello(_))), "{greeted:?}");
        assert_eq!(key_again, key, "the same worker, by its key");
        assert!(matches!(greeted_again, WorkerOutcome::Response(WorkerResponse::Hello(_))), "{greeted_again:?}");
        assert_eq!(stopped, WorkerOutcome::Response(WorkerResponse::Ok));
        assert!(exited.success(), "{exited:?}");
        let log = std::fs::read_to_string(dir.path().join("worker.log")).unwrap();
        assert!(log.contains("\"event\":\"hub.admitted\""), "{log}");
    });
}
