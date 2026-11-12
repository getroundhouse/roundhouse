//! A worker that can outlive its hub, reached through `worker --attach`, as the local provider has the hub reach it.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};

use process::ProcessRecord;
use protocol::frame;
use protocol::paths::{WORKER_RECORD_FILE, WORKER_SOCKET_FILE};
use protocol::worker::{
    WORKER_PROTOCOL_VERSION, WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse,
};

use crate::support::{is_alive, wait_until};

/// What a hub runs to reach the worker, with the worker's stream on its stdin and stdout.
struct Attached {
    command: Child,
    replies: BufReader<ChildStdout>,
}

impl Attached {
    fn start(dir: &Path) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_roundhoused"))
            .args(["worker", "--attach", "--workspaces-root"])
            .arg(dir.join("workspaces"))
            .arg("--state-dir")
            .arg(dir.join("instance"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let replies = BufReader::new(command.stdout.take().unwrap());
        Self { command, replies }
    }

    /// The worker's reply, which is the next thing it sends, since it has no agents.
    fn call(&mut self, id: u64, request: WorkerRequest) -> WorkerOutcome {
        self.command.stdin.as_mut().unwrap().write_all(&frame::encode(&WorkerCall { id, request }).unwrap()).unwrap();
        let mut line = String::new();
        self.replies.read_line(&mut line).unwrap();
        match frame::decode(line.as_bytes()).unwrap() {
            WorkerFrame::Reply { id: answering, outcome } if answering == id => outcome,
            other => panic!("expected the reply to {id}, got {other:?}"),
        }
    }

    fn hello(&mut self, keep_running_without_hub: bool) {
        let hello = WorkerRequest::Hello {
            protocol_version: WORKER_PROTOCOL_VERSION,
            oldest_protocol_version: None,
            keep_running_without_hub,
        };
        let answer = self.call(1, hello);
        assert!(matches!(answer, WorkerOutcome::Response(WorkerResponse::Hello { .. })), "{answer:?}");
    }

    /// As a hub that goes does, and returns once the command has exited.
    fn close(mut self) {
        drop(self.command.stdin.take());
        assert!(self.command.wait().unwrap().success());
    }
}

/// The worker serving the instance, as it recorded itself.
fn worker_in(dir: &Path) -> ProcessRecord {
    ProcessRecord::read(&dir.join("instance").join(WORKER_RECORD_FILE)).expect("the worker's record")
}

/// As a hub restarting for an update has the next one find its workers running.
#[test]
fn a_worker_reached_through_attach_outlives_the_command_when_its_hub_asks_and_stops_when_one_asks() {
    let dir = tempfile::tempdir().unwrap();
    let mut first = Attached::start(dir.path());
    first.hello(true);
    let worker = worker_in(dir.path());
    first.close();

    assert!(is_alive(worker.pid), "it outlived the hub that started it");
    let mut next = Attached::start(dir.path());
    next.hello(true);
    assert_eq!(worker_in(dir.path()).pid, worker.pid, "the next hub reached the same worker");
    assert_eq!(next.call(2, WorkerRequest::Stop), WorkerOutcome::Response(WorkerResponse::Ok));
    next.close();
    wait_until("the worker to stop", || !is_alive(worker.pid));
    assert!(!dir.path().join("instance").join(WORKER_SOCKET_FILE).exists());
}

/// As a hub that's stopping for good, and one from before workers could outlive it, expect.
#[test]
fn a_worker_reached_through_attach_stops_with_a_hub_that_didnt_ask_it_to_keep_running() {
    let dir = tempfile::tempdir().unwrap();
    let mut attached = Attached::start(dir.path());
    attached.hello(false);
    let worker = worker_in(dir.path());

    attached.close();

    wait_until("the worker to stop", || !is_alive(worker.pid));
}
