//! A worker that dials its hub, and outlives it, with hubs played by the tests, each on a stream in memory that the
//! worker's dial takes, as it takes the connection a hub admits it on.

#![cfg(test)]
// Its agent is a shell script, which only a Unix runs. The daemon's end-to-end tests have a worker dial on Windows.
#![cfg(unix)]

use std::sync::Arc;
use std::time::Duration;

use protocol::Build;
use protocol::event::EventBody;
use protocol::frame;
use protocol::model::{AgentKind, RunId};
use protocol::worker::{
    AgentSpec, AgentStarted, Hello, Received, RunningAgent, RunningAgentsReply, WORKER_PROTOCOL_VERSION, WorkerCall,
    WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use worker::{Config, CredentialHelper, Deleter, DialFailed, serve_dialing};

const WAIT: Duration = Duration::from_secs(10);

/// How much a stream in memory holds before a write waits, plenty for any frame these send.
const STREAM_BYTES: usize = 1 << 20;

/// The worker's end of a hub's stream.
type Dialed = (ReadHalf<DuplexStream>, WriteHalf<DuplexStream>);

/// A worker in a directory of its own, whose agent runs until a file named `done` appears beside it, and which dials
/// whichever hub a test has waiting next.
struct Dialing {
    dir: tempfile::TempDir,
    hubs: mpsc::UnboundedSender<Dialed>,
    running: JoinHandle<std::io::Result<()>>,
}

impl Dialing {
    /// With no test to stop with, as a worker of the product's has.
    fn start(idle_limit: Duration) -> Self {
        Self::stopping_with(idle_limit, std::future::pending())
    }

    /// Stopping with a test once `test_gone` returns, as a test's worker does.
    fn stopping_with(idle_limit: Duration, test_gone: impl Future<Output = ()> + Send + 'static) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join("fake-cli");
        process::testing::write_program(&cli, "#!/bin/sh\nwhile [ ! -e done ]; do sleep 0.02; done\n").unwrap();
        std::fs::create_dir(dir.path().join("state")).unwrap();
        let config = Config {
            build: Build::UNSTAMPED,
            workspaces_root: dir.path().to_path_buf(),
            state_dir: dir.path().join("state"),
            clones_dir: dir.path().join("clones"),
            home: None,
            path: None,
            claude_bin: Some(cli),
            codex_bin: None,
            antigravity_bin: None,
            scratch_dir: dir.path().to_path_buf(),
            deleter: Deleter::new("/bin/rm", ["-rf", "--"]),
            // Never run here, since nothing these start asks git for a credential.
            credential_helper: CredentialHelper::new("/bin/false", ["--"]),
            machine: process::testing::git_environment().given(),
            refused: None,
            timers: worker::Timers::DEFAULT,
        };
        let (hubs, waiting) = mpsc::unbounded_channel::<Dialed>();
        let waiting = Arc::new(Mutex::new(waiting));
        let dial = move || {
            let waiting = waiting.clone();
            async move {
                let next = waiting.lock().await.recv().await;
                next.ok_or_else(|| DialFailed::Unreachable("no hub is coming".into()))
            }
        };
        let running = tokio::spawn(serve_dialing(dial, config, idle_limit, test_gone));
        Self { dir, hubs, running }
    }

    /// A hub the worker dials next, once it dials.
    fn hub(&self) -> Hub {
        let (hub_end, worker_end) = tokio::io::duplex(STREAM_BYTES);
        self.hubs.send(tokio::io::split(worker_end)).unwrap();
        let (frames, calls) = tokio::io::split(hub_end);
        Hub { calls, frames: BufReader::new(frames).lines(), next_id: 1 }
    }

    fn start_spec(&self) -> WorkerRequest {
        WorkerRequest::AgentStart(AgentSpec {
            session_id: "s".into(),
            run: RunId::new(4),
            agent: AgentKind::Claude,
            cwd: self.dir.path().to_string_lossy().into_owned(),
            start: protocol::capability::conversation::Start::New { conversation: Some("a".into()) },
            settings: claude_defaults(),
            env: vec![],
            tools_token: None,
            login: None,
            repository: None,
            interaction: protocol::model::InteractionMode::Typing,
        })
    }

    async fn stopped(self) {
        timeout(WAIT, self.running).await.expect("the worker to stop").unwrap().unwrap();
    }
}

/// A hub, played by the test, on the stream the worker dialed.
struct Hub {
    calls: WriteHalf<DuplexStream>,
    frames: Lines<BufReader<ReadHalf<DuplexStream>>>,
    next_id: u64,
}

impl Hub {
    /// Says hello, and returns whatever the worker sent after its reply to that, up to its reply to a call that lists
    /// its agents, with that list.
    async fn greet(mut self, keep_running_without_hub: bool) -> (Self, Vec<WorkerFrame>, Vec<RunningAgent>) {
        let hello = WorkerRequest::Hello(Hello {
            protocol_version: WORKER_PROTOCOL_VERSION,
            oldest_protocol_version: None,
            keep_running_without_hub,
        });
        let (greeted, _) = self.call(hello).await;
        assert!(matches!(greeted, WorkerOutcome::Response(WorkerResponse::Hello(_))), "{greeted:?}");
        let (listed, before) = self.call(WorkerRequest::RunningAgents).await;
        let WorkerOutcome::Response(WorkerResponse::RunningAgents(RunningAgentsReply { agents })) = listed else {
            panic!("{listed:?}")
        };
        (self, before, agents)
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
/// next hub the worker dials gets what the agents said meanwhile, until a hub says it has recorded it.
#[tokio::test]
async fn a_worker_asked_to_keep_running_carries_on_without_its_hub_and_sends_the_next_what_it_missed() {
    let worker = Dialing::start(WAIT);
    let (mut first, _, _) = worker.hub().greet(true).await;
    let (started, _) = first.call(worker.start_spec()).await;
    assert!(matches!(started, WorkerOutcome::Response(WorkerResponse::AgentStarted(_))), "{started:?}");
    first.go().await;

    let (mut second, _, agents) = worker.hub().greet(true).await;
    assert_eq!(agents.len(), 1, "it outlived the first hub: {agents:?}");
    assert_eq!((&agents[0].session_id, agents[0].run), (&"s".into(), RunId::new(4)));
    let claude = agents::claude::description().capabilities;
    assert_eq!(*agents[0].capabilities, claude, "it says what the run claims, for a hub that didn't hear it start");
    std::fs::write(worker.dir.path().join("done"), "").unwrap();
    // Never recorded, since this hub goes without saying so.
    let missed = second.until_exited().await;
    second.go().await;
    let (mut third, replayed, agents) = worker.hub().greet(true).await;
    let (received, _) = third.call(WorkerRequest::Received(Received { through: 1 })).await;
    third.go().await;
    let (mut fourth, after_recording, _) = worker.hub().greet(true).await;
    let (stopped, _) = fourth.call(WorkerRequest::Stop).await;
    fourth.go().await;

    assert!(agents.is_empty(), "{agents:?}");
    assert_eq!((exited(&missed), exited(&replayed)), (vec![Some(1)], vec![Some(1)]), "numbered, and sent again");
    assert_eq!(received, WorkerOutcome::Response(WorkerResponse::Ok));
    assert!(after_recording.is_empty(), "{after_recording:?}");
    assert_eq!(stopped, WorkerOutcome::Response(WorkerResponse::Ok));
    worker.stopped().await;
}

/// As a hub that's stopping for good expects, and one from before workers could outlive it.
#[tokio::test]
async fn a_hub_that_didnt_ask_the_worker_to_keep_running_stops_it_by_going() {
    let worker = Dialing::start(WAIT);
    let (mut hub, _, _) = worker.hub().greet(false).await;
    hub.call(worker.start_spec()).await;

    let rest = hub.go().await;

    assert_eq!(exited(&rest), [None], "it stopped the agent, and sent on what it said as it did, unnumbered");
    worker.stopped().await;
}

/// So a hub that was killed doesn't leave agents running unrecorded for good: once the worker has had no hub for its
/// limit, or, for a test's worker, once the test has gone, as a run that's killed before the test stops them leaves it.
#[tokio::test]
async fn a_worker_left_without_a_hub_stops_its_agents_in_time_or_with_its_test() {
    for with_its_test in [false, true] {
        let (test, test_going) = tokio::sync::oneshot::channel::<()>();
        let worker = if with_its_test {
            Dialing::stopping_with(Duration::from_mins(10), async move { drop(test_going.await) })
        } else {
            Dialing::start(Duration::from_millis(500))
        };
        let (mut hub, _, _) = worker.hub().greet(true).await;
        let (started, _) = hub.call(worker.start_spec()).await;
        let WorkerOutcome::Response(WorkerResponse::AgentStarted(AgentStarted { pid, .. })) = started else {
            panic!("{started:?}")
        };

        hub.go().await;
        drop(test);
        worker.stopped().await;

        let alive = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success();
        assert!(!alive, "the agent outlived its worker, with its test {with_its_test}");
    }
}

/// What a Claude chat starts with, as a launch says it.
fn claude_defaults() -> protocol::capability::settings::Settings {
    agents::claude::description().defaults()
}
