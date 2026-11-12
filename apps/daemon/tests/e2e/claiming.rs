//! One worker per instance, however many times a provider's `connect` is asked.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use protocol::client::ErrorCode;
use protocol::frame;
use protocol::worker::WORKER_PROTOCOL_VERSION;
use protocol::worker::{WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest};

fn worker(dir: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_roundhoused"))
        .args(["worker", "--stdio", "--workspaces-root"])
        .arg(dir.join("workspaces"))
        .arg("--state-dir")
        .arg(dir.join("instance"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

/// Says hello, and returns the answer. A worker sends nothing else until it's asked for more.
fn hello(worker: &mut Child) -> WorkerOutcome {
    let request = WorkerRequest::Hello { protocol_version: WORKER_PROTOCOL_VERSION, oldest_protocol_version: None };
    let call = WorkerCall { id: 1, request };
    worker.stdin.as_mut().unwrap().write_all(&frame::encode(&call).unwrap()).unwrap();
    let mut line = String::new();
    BufReader::new(worker.stdout.as_mut().unwrap()).read_line(&mut line).unwrap();
    match frame::decode(line.as_bytes()).unwrap() {
        WorkerFrame::Reply { id: 1, outcome } => outcome,
        other => panic!("expected the reply to hello, got {other:?}"),
    }
}

/// Exiting instead would look to the hub like a worker that failed to start, and the hub cleans up after one of those
/// by ending its instance, with the worker serving it.
#[test]
fn a_second_worker_on_an_instance_says_it_is_taken_and_leaves_the_first_serving() {
    let dir = tempfile::tempdir().unwrap();
    let mut first = worker(dir.path());
    // It claims the instance before it answers anything.
    assert!(matches!(hello(&mut first), WorkerOutcome::Response(_)));
    let mut second = worker(dir.path());

    let refused = hello(&mut second);

    assert!(matches!(&refused, WorkerOutcome::Error(e) if e.code == ErrorCode::Conflict), "{refused:?}");
    assert!(second.try_wait().unwrap().is_none(), "it should wait for its stream to close");
    drop(second.stdin.take());
    assert!(second.wait().unwrap().success());
    assert!(matches!(hello(&mut first), WorkerOutcome::Response(_)), "the first should still be serving");
    drop(first.stdin.take());
    assert!(first.wait().unwrap().success());
}
