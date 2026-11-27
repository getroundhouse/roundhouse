//! A worker that dials its hub with a ticket, as a provider starts it, against a stand-in for the hub's endpoint that
//! admits it by hand.

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

/// In a process group of its own, as a provider starts one, so ending it ends what it started too.
fn start_worker(dir: &Path, ticket: &WorkerTicket) -> Child {
    use std::os::unix::process::CommandExt as _;
    WholeEnvironment::for_a_test(&dir.join("home"))
        .command(crate::support::daemon())
        .process_group(0)
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

/// A stand-in for the next hub, which binds the endpoint a hub's own machine's workers dial, with the key and the port
/// the hub in `data_dir` keeps, so the worker an earlier hub left dials it again, as it would the next hub, and is
/// admitted, whatever it says, and greeted as a hub that leaves it running would.
pub(crate) struct NextHub {
    runtime: tokio::runtime::Runtime,
    hub: HubEndpoint,
    _connection: dialing::Connection,
    send: dialing::SendStream,
    recv: dialing::RecvStream,
}

impl NextHub {
    pub(crate) fn admitting(data_dir: &Path) -> Self {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let key_file = data_dir.join(protocol::paths::LOOPBACK_WORKER_ENDPOINT_FILE);
        let (hub, connection, mut send, mut recv) = runtime.block_on(async {
            let hub = HubEndpoint::bind(&key_file, Relays::None).await.unwrap();
            let (connection, send, recv, _, _) = tokio::time::timeout(TIMEOUT, admit(&hub)).await.unwrap();
            (hub, connection, send, recv)
        });
        let greeted = runtime.block_on(call(&mut send, &mut recv, 1, hello()));
        assert!(matches!(greeted, WorkerOutcome::Response(WorkerResponse::Hello(_))), "{greeted:?}");
        Self { runtime, hub, _connection: connection, send, recv }
    }

    /// The worker's reply to `request`, past whatever else it sends first, such as what its watches find.
    pub(crate) fn call(&mut self, id: u64, request: WorkerRequest) -> WorkerOutcome {
        let (send, recv) = (&mut self.send, &mut self.recv);
        self.runtime.block_on(async { tokio::time::timeout(TIMEOUT, call(send, recv, id, request)).await.unwrap() })
    }

    /// What `wanted` finds in the first frame it finds anything in, of those the worker sends from now on.
    pub(crate) fn frame_until<T>(&mut self, what: &str, wanted: impl Fn(WorkerFrame) -> Option<T>) -> T {
        let recv = &mut self.recv;
        let found = self.runtime.block_on(async {
            tokio::time::timeout(TIMEOUT, async {
                loop {
                    if let Some(found) = wanted(read_frame::<WorkerFrame>(recv).await.unwrap()) {
                        return found;
                    }
                }
            })
            .await
        });
        found.unwrap_or_else(|_| panic!("timed out waiting for {what}"))
    }

    /// As a hub that goes does, letting go of the endpoint, so the next one binds it.
    pub(crate) fn close(mut self) {
        let _ = self.send.finish();
        self.runtime.block_on(self.hub.close());
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

/// As a provider starts one in a VM, from a command whose session ends with it: the command returns at once, and the
/// worker it started dials from the port it's given, and keeps its log.
#[test]
fn a_worker_started_detached_returns_at_once_and_dials_from_its_port() {
    let dir = tempfile::tempdir().unwrap();
    let port = std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let hub = HubEndpoint::bind(&dir.path().join("endpoint.json"), Relays::None).await.unwrap();
        let ticket = WorkerTicket::from(&Ticket {
            hub: hub.address().await,
            secret: TicketSecret::from("the-secret".to_string()),
        });

        let started = WholeEnvironment::for_a_test(&dir.path().join("home"))
            .command(env!("CARGO_BIN_EXE_roundhoused"))
            .args(["worker", "--ticket", ticket.as_str(), "--port", &port.to_string(), "--advertise"])
            .arg(format!("127.0.0.1:{port}"))
            .arg("--detach")
            .arg("--log")
            .arg(dir.path().join("worker.log"))
            .arg("--workspaces-root")
            .arg(dir.path().join("workspaces"))
            .arg("--state-dir")
            .arg(dir.path().join("instance"))
            .arg("--clones-dir")
            .arg(dir.path().join("clones"))
            .output()
            .unwrap();
        let (_connection, mut send, mut recv, _, _) = tokio::time::timeout(TIMEOUT, admit(&hub)).await.unwrap();
        let bound = std::net::UdpSocket::bind(("127.0.0.1", port));
        let stopped = call(&mut send, &mut recv, 1, WorkerRequest::Stop).await;
        send.finish().unwrap();

        assert!(started.status.success(), "{started:?}");
        assert!(bound.is_err(), "the worker holds the port it was given");
        assert_eq!(stopped, WorkerOutcome::Response(WorkerResponse::Ok));
        assert!(std::fs::read_to_string(dir.path().join("worker.log")).unwrap().contains("\"event\":\"hub.dialing\""));
    });
}

/// So a provider ends a worker that dials, and what it left, as the worker finds them, rather than by a script of its
/// own.
#[test]
fn end_worker_ends_the_worker_serving_its_state_directory() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let hub = runtime.block_on(HubEndpoint::bind(&dir.path().join("endpoint.json"), Relays::None)).unwrap();
    let secret = TicketSecret::from("the-secret".to_string());
    let ticket = WorkerTicket::from(&Ticket { hub: runtime.block_on(hub.address()), secret });
    let mut worker = start_worker(dir.path(), &ticket);
    let _admitted = runtime.block_on(async { tokio::time::timeout(TIMEOUT, admit(&hub)).await.unwrap() });

    let ended = WholeEnvironment::for_a_test(&dir.path().join("home"))
        .command(env!("CARGO_BIN_EXE_roundhoused"))
        .args(["end-worker", "--now", "--state-dir"])
        .arg(dir.path().join("instance"))
        .output()
        .unwrap();

    assert!(ended.status.success(), "{ended:?}");
    let exited = waiting::thread::until_ok(TIMEOUT, || worker.try_wait().unwrap().ok_or("the worker is still running"));
    assert!(!exited.success(), "it was killed: {exited:?}");
}
