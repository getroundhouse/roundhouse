//! A worker that can outlive its hub, reached through `worker --attach`, as the local provider has the hub reach it.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::mpsc;
use std::time::Instant;

use process::ProcessRecord;
use process::testing::WholeEnvironment;
use protocol::frame;
use protocol::paths::{WORKER_RECORD_FILE, WORKER_SOCKET_FILE};
use protocol::worker::{
    Hello, WORKER_PROTOCOL_VERSION, WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse,
};

use crate::support::{TIMEOUT, is_alive, wait_until};

/// What a hub runs to reach the worker, with the worker's stream on its stdin and stdout.
pub(crate) struct Attached {
    command: Child,
    /// Each line the worker sends, read on a thread of its own, so a wait for one gives up rather than waiting for ever.
    lines: mpsc::Receiver<String>,
}

impl Attached {
    fn start(dir: &Path) -> Self {
        Self::start_in(&dir.join("workspaces"), &dir.join("instance"))
    }

    /// To the worker whose state directory is `state_dir`, starting one if none is serving it.
    fn start_in(workspaces_root: &Path, state_dir: &Path) -> Self {
        let mut command = WholeEnvironment::for_a_test(&state_dir.with_file_name("home"))
            .command(crate::support::daemon())
            .args(["worker", "--attach", "--workspaces-root"])
            .arg(workspaces_root)
            .arg("--state-dir")
            .arg(state_dir)
            .arg("--clones-dir")
            .arg(workspaces_root.with_file_name("clones"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let (read, lines) = mpsc::channel();
        let sent = BufReader::new(command.stdout.take().unwrap());
        std::thread::spawn(move || {
            for line in sent.lines().map_while(Result::ok) {
                if read.send(line).is_err() {
                    return;
                }
            }
        });
        Self { command, lines }
    }

    /// The worker's reply, which has to be the next thing it sends but for what its watches find of the branches. A
    /// watch a hub before asked for goes on sending to whichever hub is connected whenever the branches change, so
    /// that can come at any time, and says nothing of what the worker kept.
    pub(crate) fn call(&mut self, id: u64, request: WorkerRequest) -> WorkerOutcome {
        self.command.stdin.as_mut().unwrap().write_all(&frame::encode(&WorkerCall { id, request }).unwrap()).unwrap();
        let not_the_branches = |frame| {
            (!matches!(frame, WorkerFrame::Branches { .. } | WorkerFrame::ChildBranches { .. })).then_some(frame)
        };
        match self.frame_until(&format!("the reply to {id}"), not_the_branches) {
            WorkerFrame::Reply { id: answering, outcome } if answering == id => outcome,
            other => panic!("expected the reply to {id}, got {other:?}"),
        }
    }

    /// What `wanted` finds in the first frame it finds anything in, of those the worker sends from now on.
    pub(crate) fn frame_until<T>(&mut self, what: &str, wanted: impl Fn(WorkerFrame) -> Option<T>) -> T {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let line = self.lines.recv_timeout(deadline.saturating_duration_since(Instant::now()));
            let line = line.unwrap_or_else(|e| panic!("timed out waiting for {what}: {e}"));
            if let Some(found) = wanted(frame::decode(line.as_bytes()).unwrap()) {
                return found;
            }
        }
    }

    pub(crate) fn hello(&mut self, keep_running_without_hub: bool) {
        let hello = WorkerRequest::Hello(Hello {
            protocol_version: WORKER_PROTOCOL_VERSION,
            oldest_protocol_version: None,
            keep_running_without_hub,
        });
        let answer = self.call(1, hello);
        assert!(matches!(answer, WorkerOutcome::Response(WorkerResponse::Hello(_))), "{answer:?}");
    }

    /// As the next hub reaches the worker whose state directory is `state_dir`.
    pub(crate) fn next_hub(workspaces_root: &Path, state_dir: &Path) -> Self {
        let mut next = Self::start_in(workspaces_root, state_dir);
        next.hello(true);
        next
    }

    /// As a hub that goes does, and returns once the command has exited.
    pub(crate) fn close(mut self) {
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

/// As a hub that's stopping for good expects.
#[test]
fn a_worker_reached_through_attach_stops_with_a_hub_that_didnt_ask_it_to_keep_running() {
    let dir = tempfile::tempdir().unwrap();
    let mut attached = Attached::start(dir.path());
    attached.hello(false);
    let worker = worker_in(dir.path());

    attached.close();

    wait_until("the worker to stop", || !is_alive(worker.pid));
}
