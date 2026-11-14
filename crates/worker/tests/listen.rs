//! A worker that can outlive its hub, served on a Unix socket, with hubs played by the tests.

#![cfg(test)]
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use protocol::Build;
use protocol::client::ErrorCode;
use protocol::event::EventBody;
use protocol::frame;
use protocol::model::{AgentKind, RunId};
use protocol::worker::{
    AgentSpec, AgentStarted, Hello, Received, RunningAgent, RunningAgentsReply, WORKER_PROTOCOL_VERSION, WorkerCall,
    WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use worker::{Config, Deleter, listen};

const WAIT: Duration = Duration::from_secs(10);

/// A worker listening in a directory of its own, whose agent runs until a file named `done` appears beside it.
struct Listening {
    dir: tempfile::TempDir,
    socket: PathBuf,
    running: JoinHandle<std::io::Result<()>>,
}

impl Listening {
    fn start(idle_limit: Duration) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join("fake-cli");
        std::fs::write(&cli, "#!/bin/sh\nwhile [ ! -e done ]; do sleep 0.02; done\n").unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = Config {
            build: Build::UNSTAMPED,
            workspaces_root: dir.path().to_path_buf(),
            state_dir: None,
            home: None,
            path: None,
            claude_bin: Some(cli),
            scratch_dir: dir.path().to_path_buf(),
            deleter: Deleter::new("/bin/rm", ["-rf", "--"]),
            refused: None,
        };
        let socket = dir.path().join("worker.sock");
        let path = socket.clone();
        let running = tokio::spawn(async move { listen(&path, config, idle_limit).await });
        Self { dir, socket, running }
    }

    fn start_spec(&self) -> WorkerRequest {
        WorkerRequest::AgentStart(AgentSpec {
            session_id: "s".into(),
            run: RunId::new(4),
            agent: AgentKind::Claude,
            cwd: self.dir.path().to_string_lossy().into_owned(),
            start: protocol::capability::conversation::Start::New { conversation: Some("a".into()) },
            settings: agents::claude::description().defaults,
            env: vec![],
            tools_token: None,
            login: None,
        })
    }

    async fn stopped(self) {
        timeout(WAIT, self.running).await.expect("the worker to stop").unwrap().unwrap();
    }
}

/// A hub, played by the test, connected to the worker.
struct Hub {
    calls: OwnedWriteHalf,
    frames: Lines<BufReader<OwnedReadHalf>>,
    next_id: u64,
}

impl Hub {
    async fn connect(socket: &Path) -> Self {
        let stream = timeout(WAIT, async {
            loop {
                match UnixStream::connect(socket).await {
                    Ok(stream) => return stream,
                    Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .expect("the worker's socket");
        let (frames, calls) = stream.into_split();
        Self { calls, frames: BufReader::new(frames).lines(), next_id: 1 }
    }

    /// Connects, says hello, and returns whatever the worker sent after its reply to that, up to its reply to a call
    /// that lists its agents, with that list.
    async fn greet(socket: &Path, keep_running_without_hub: bool) -> (Self, Vec<WorkerFrame>, Vec<RunningAgent>) {
        let mut hub = Self::connect(socket).await;
        let hello = WorkerRequest::Hello(Hello {
            protocol_version: WORKER_PROTOCOL_VERSION,
            oldest_protocol_version: None,
            keep_running_without_hub,
        });
        assert!(matches!(hub.call(hello).await.0, WorkerOutcome::Response(WorkerResponse::Hello(_))));
        let (listed, before) = hub.call(WorkerRequest::RunningAgents).await;
        let WorkerOutcome::Response(WorkerResponse::RunningAgents(RunningAgentsReply { agents })) = listed else {
            panic!("{listed:?}")
        };
        (hub, before, agents)
    }

    /// The worker's reply to `request`, and what it sent before it.
    async fn call(&mut self, request: WorkerRequest) -> (WorkerOutcome, Vec<WorkerFrame>) {
        let id = self.next_id;
        self.next_id += 1;
        self.calls.write_all(&frame::encode(&WorkerCall { id, request }).unwrap()).await.unwrap();
        let mut before = Vec::new();
        loop {
            match self.next().await.expect("a reply") {
                WorkerFrame::Reply { id: answering, outcome } if answering == id => return (outcome, before),
                other => before.push(other),
            }
        }
    }

    async fn next(&mut self) -> Option<WorkerFrame> {
        let line = timeout(WAIT, self.frames.next_line()).await.expect("a frame from the worker").unwrap()?;
        Some(frame::decode(line.as_bytes()).unwrap())
    }

    /// What the worker sends up to the agent's exit.
    async fn until_exited(&mut self) -> Vec<WorkerFrame> {
        let mut sent = Vec::new();
        while exited(&sent).is_empty() {
            sent.push(self.next().await.expect("the agent's exit"));
        }
        sent
    }

    /// Goes, with nothing more to say, and reads whatever the worker still sends.
    async fn go(mut self) -> Vec<WorkerFrame> {
        self.calls.shutdown().await.unwrap();
        let mut rest = Vec::new();
        while let Some(frame) = self.next().await {
            rest.push(frame);
        }
        rest
    }
}

fn exited(frames: &[WorkerFrame]) -> Vec<Option<u64>> {
    let exit = |frame: &WorkerFrame| match frame {
        WorkerFrame::AgentEvent { body: EventBody::AgentExited { .. }, seq, .. } => Some(*seq),
        _ => None,
    };
    frames.iter().filter_map(exit).collect()
}

/// A hub restarting, as for an update, asks its workers to keep running without it, so their agents carry on, and the
/// next hub gets what the agents said meanwhile, until a hub says it has recorded it.
#[tokio::test]
async fn a_worker_asked_to_keep_running_carries_on_without_its_hub_and_sends_the_next_what_it_missed() {
    let worker = Listening::start(WAIT);
    let (mut first, _, _) = Hub::greet(&worker.socket, true).await;
    let (started, _) = first.call(worker.start_spec()).await;
    assert!(matches!(started, WorkerOutcome::Response(WorkerResponse::AgentStarted(_))), "{started:?}");
    first.go().await;

    let (mut second, _, agents) = Hub::greet(&worker.socket, true).await;
    assert_eq!(agents, [RunningAgent { session_id: "s".into(), run: RunId::new(4) }], "it outlived the first hub");
    std::fs::write(worker.dir.path().join("done"), "").unwrap();
    // Never recorded, since this hub goes without saying so.
    let missed = second.until_exited().await;
    second.go().await;
    let (mut third, replayed, agents) = Hub::greet(&worker.socket, true).await;
    let (received, _) = third.call(WorkerRequest::Received(Received { through: 1 })).await;
    third.go().await;
    let (mut fourth, after_recording, _) = Hub::greet(&worker.socket, true).await;
    let (stopped, _) = fourth.call(WorkerRequest::Stop).await;
    fourth.go().await;

    assert!(agents.is_empty(), "{agents:?}");
    assert_eq!((exited(&missed), exited(&replayed)), (vec![Some(1)], vec![Some(1)]), "numbered, and sent again");
    assert_eq!(received, WorkerOutcome::Response(WorkerResponse::Ok));
    assert!(after_recording.is_empty(), "{after_recording:?}");
    assert_eq!(stopped, WorkerOutcome::Response(WorkerResponse::Ok));
    worker.stopped().await;
}

/// As a hub from before workers could outlive it, and one that's stopping for good, expect.
#[tokio::test]
async fn a_hub_that_didnt_ask_the_worker_to_keep_running_stops_it_by_going() {
    let worker = Listening::start(WAIT);
    let (mut hub, _, _) = Hub::greet(&worker.socket, false).await;
    hub.call(worker.start_spec()).await;

    let rest = hub.go().await;

    assert_eq!(exited(&rest), [None], "it stopped the agent, and sent on what it said as it did, unnumbered");
    assert!(!worker.socket.exists(), "a hub connecting next starts another worker");
    worker.stopped().await;
}

/// So no other user can tell the worker what to run.
#[tokio::test]
async fn only_this_user_can_connect_to_the_worker() {
    let worker = Listening::start(WAIT);
    let (hub, _, _) = Hub::greet(&worker.socket, false).await;

    let mode = std::fs::metadata(&worker.socket).unwrap().permissions().mode() & 0o777;

    assert_eq!(mode, 0o600);
    hub.go().await;
    worker.stopped().await;
}

/// So a hub that asks twice for its connection to the worker learns there's one already, rather than taking the worker
/// from itself.
#[tokio::test]
async fn a_second_hub_is_refused_while_one_is_connected() {
    let worker = Listening::start(WAIT);
    let (mut first, _, _) = Hub::greet(&worker.socket, true).await;
    let mut second = Hub::connect(&worker.socket).await;

    let (refused, _) = second.call(WorkerRequest::RunningAgents).await;
    let (still, _) = first.call(WorkerRequest::RunningAgents).await;

    assert!(matches!(&refused, WorkerOutcome::Error(e) if e.code == ErrorCode::Conflict), "{refused:?}");
    assert!(matches!(still, WorkerOutcome::Response(WorkerResponse::RunningAgents(_))), "{still:?}");
    first.call(WorkerRequest::Stop).await;
    first.go().await;
    worker.stopped().await;
}

/// So a hub that was killed doesn't leave agents running unrecorded for good.
#[tokio::test]
async fn a_worker_left_without_a_hub_stops_its_agents_in_time() {
    let worker = Listening::start(Duration::from_millis(500));
    let (mut hub, _, _) = Hub::greet(&worker.socket, true).await;
    let (started, _) = hub.call(worker.start_spec()).await;
    let WorkerOutcome::Response(WorkerResponse::AgentStarted(AgentStarted { pid, .. })) = started else {
        panic!("{started:?}")
    };

    hub.go().await;
    worker.stopped().await;

    let alive = std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success();
    assert!(!alive, "the agent outlived its worker");
}
