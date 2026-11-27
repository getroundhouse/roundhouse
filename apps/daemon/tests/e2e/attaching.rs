//! A worker an earlier build left listening on a socket, reached through `worker --attach`, as the local provider has
//! the hub reach one, so an update doesn't end its agents. This build starts none, so a stand-in is that worker.

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
struct Attached {
    command: Child,
    /// Each line the worker sends, read on a thread of its own, so a wait for one gives up rather than waiting for ever.
    lines: mpsc::Receiver<String>,
}

impl Attached {
    /// To the worker whose state directory is `state_dir`, without starting one.
    fn attach_in(workspaces_root: &Path, state_dir: &Path) -> Self {
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
    fn call(&mut self, id: u64, request: WorkerRequest) -> WorkerOutcome {
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
    fn frame_until<T>(&mut self, what: &str, wanted: impl Fn(WorkerFrame) -> Option<T>) -> T {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let line = self.lines.recv_timeout(deadline.saturating_duration_since(Instant::now()));
            let line = line.unwrap_or_else(|e| panic!("timed out waiting for {what}: {e}"));
            if let Some(found) = wanted(frame::decode(line.as_bytes()).unwrap()) {
                return found;
            }
        }
    }

    fn hello(&mut self, keep_running_without_hub: bool) {
        let hello = WorkerRequest::Hello(Hello {
            protocol_version: WORKER_PROTOCOL_VERSION,
            oldest_protocol_version: None,
            keep_running_without_hub,
        });
        let answer = self.call(1, hello);
        assert!(matches!(answer, WorkerOutcome::Response(WorkerResponse::Hello(_))), "{answer:?}");
    }

    /// As a hub that goes does, and returns once the command has exited.
    fn close(mut self) {
        drop(self.command.stdin.take());
        assert!(self.command.wait().unwrap().success());
    }
}

/// A stand-in for a worker an earlier build left listening on a socket in `state_dir`, as that build's `--attach`
/// started it, once it listens and its worker has recorded itself. Its worker runs `daemon`, in a group of its own, and
/// the stand-in goes once the worker has.
pub(crate) fn start_listening(daemon: &Path, workspaces_root: &Path, state_dir: &Path) {
    use std::os::unix::process::CommandExt as _;
    let mut started = WholeEnvironment::for_a_test(&state_dir.with_file_name("home"))
        .command(crate::support::earlier_worker())
        .arg(daemon)
        .arg("--workspaces-root")
        .arg(workspaces_root)
        .arg("--state-dir")
        .arg(state_dir)
        .arg("--clones-dir")
        .arg(workspaces_root.with_file_name("clones"))
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Should it stop while the test runs, so it isn't left a zombie.
    std::thread::spawn(move || started.wait());
    wait_until("the worker to listen", || {
        state_dir.join(WORKER_SOCKET_FILE).exists()
            && ProcessRecord::read(&state_dir.join(WORKER_RECORD_FILE)).is_some()
    });
}

/// The worker serving the instance, as it recorded itself.
fn worker_in(dir: &Path) -> ProcessRecord {
    ProcessRecord::read(&dir.join("instance").join(WORKER_RECORD_FILE)).expect("the worker's record")
}

/// So the first hub of a build whose workers dial reaches one an earlier build left, and can stop it.
#[test]
fn attach_reaches_a_worker_an_earlier_build_left_listening() {
    let dir = tempfile::tempdir().unwrap();
    let (workspaces, instance) = (dir.path().join("workspaces"), dir.path().join("instance"));
    start_listening(crate::support::daemon(), &workspaces, &instance);
    let worker = worker_in(dir.path());

    let mut attached = Attached::attach_in(&workspaces, &instance);
    attached.hello(true);
    let stopped = attached.call(2, WorkerRequest::Stop);
    attached.close();

    assert_eq!(stopped, WorkerOutcome::Response(WorkerResponse::Ok));
    wait_until("the worker to stop", || !is_alive(worker.pid));
}

/// Nothing starts a worker that listens any more, since one that outlives its hub dials it.
#[test]
fn attach_starts_no_worker_where_none_listens() {
    let dir = tempfile::tempdir().unwrap();
    let attached = Attached::attach_in(&dir.path().join("workspaces"), &dir.path().join("instance"));

    let mut command = attached.command;
    let exited = command.wait().unwrap();

    assert!(!exited.success(), "{exited:?}");
    assert!(!dir.path().join("instance").join(WORKER_RECORD_FILE).exists(), "no worker started");
}
