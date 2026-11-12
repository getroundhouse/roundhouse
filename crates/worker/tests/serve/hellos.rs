//! What a worker says to a hub of another version.

use protocol::StampedBuild;
use protocol::client::ErrorCode;
use protocol::frame;
use protocol::worker::{WORKER_PROTOCOL_VERSION, WorkerCall, WorkerOutcome, WorkerRequest, WorkerResponse};
use worker::Config;

use super::{config, hello, replies_to};

/// From a newer hub: one it doesn't know, and a known one with a value it can't read. Either is refused, so the hub
/// does without it, rather than left for the hub's call to wait on until the connection closes.
#[tokio::test]
async fn a_request_from_a_newer_hub_is_refused_and_the_worker_carries_on() {
    let dir = tempfile::tempdir().unwrap();
    let calls = [
        frame::encode(&WorkerCall { id: 0, request: hello() }).unwrap(),
        b"{\"id\":1,\"request\":{\"method\":\"from_a_newer_hub\",\"path\":\"a\"}}\n".to_vec(),
        b"{\"id\":2,\"request\":{\"method\":\"account_for\",\"agent\":{\"newer\":true}}}\n".to_vec(),
        frame::encode(&WorkerCall { id: 3, request: hello() }).unwrap(),
    ];

    let answered = replies_to(config(dir.path(), None), &calls).await;

    for id in [1, 2] {
        assert!(matches!(&answered[&id], WorkerOutcome::Error(e) if e.code == ErrorCode::Unsupported), "{answered:?}");
    }
    assert!(matches!(answered[&3], WorkerOutcome::Response(WorkerResponse::Hello { .. })), "it carried on");
}

/// A worker refused its instance answers every request by saying so, one it can't read included, so the hub can tell
/// it from a worker that failed to start.
#[tokio::test]
async fn a_refused_worker_says_so_even_to_a_request_it_cant_read() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config { refused: Some("another worker is serving it".into()), ..config(dir.path(), None) };
    let calls = [b"{\"id\":1,\"request\":{\"method\":\"account_for\",\"agent\":{\"newer\":true}}}\n".to_vec()];

    let answered = replies_to(config, &calls).await;

    assert!(matches!(&answered[&1], WorkerOutcome::Error(e) if e.code == ErrorCode::Conflict), "{answered:?}");
}

/// A hub speaks the version before its own too, for 30 days after a break, and a worker takes one whose hello names
/// its own version, but not one that no longer speaks it. It says which build it was given.
#[tokio::test]
async fn a_worker_takes_a_hub_whose_hello_names_its_version_in_a_range() {
    let dir = tempfile::tempdir().unwrap();
    let ours = WORKER_PROTOCOL_VERSION;
    let hello = |protocol_version, oldest_protocol_version| WorkerRequest::Hello {
        protocol_version,
        oldest_protocol_version,
        keep_running_without_hub: false,
    };
    let hellos = [hello(ours + 1, Some(ours)), hello(ours + 2, Some(ours + 1)), hello(ours, None)];
    let calls: Vec<_> = hellos
        .into_iter()
        .enumerate()
        .map(|(id, request)| frame::encode(&WorkerCall { id: id as u64, request }).unwrap())
        .collect();
    let config = Config { build: StampedBuild::new("given", 1).unwrap().into(), ..config(dir.path(), None) };

    let answered = replies_to(config, &calls).await;

    let build = Some("given".to_string());
    let chosen = WorkerOutcome::Response(WorkerResponse::Hello { protocol_version: ours, build });
    assert_eq!(answered[&0], chosen, "a newer hub that still speaks this worker's version");
    assert!(matches!(&answered[&1], WorkerOutcome::Error(e) if e.code == ErrorCode::VersionMismatch), "{answered:?}");
    assert_eq!(answered[&2], chosen, "a hub from before ranges");
}
