//! Replaying a capture of a scenario through the adapter's own process code, with a stand-in for the CLI.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::Mutex;
use protocol::model::RunId;
use tokio::sync::mpsc;

use crate::conformance::capture::{self, Entry};
use crate::conformance::driver::{Driver, Launcher};
use crate::conformance::lines::Lines;
use crate::conformance::scenario::{Failed, Scenario};
use crate::conformance::stand_in::{QUIET, StandIn};
use crate::conformance::trial::{Played, Report, Trial};
use crate::{AgentOutput, AgentProcess};
use protocol::model::AgentSessionId;

/// The agent-specific launch and wire dialect, with all replay orchestration shared.
#[async_trait]
pub trait Binding: Send + Sync {
    fn lines(&self) -> &'static dyn Lines;
    async fn spawn(
        &self,
        launch: Launch<'_>,
        output: mpsc::Sender<AgentOutput>,
    ) -> Result<Arc<dyn AgentProcess>, Failed>;
}

pub struct Launch<'a> {
    pub scenario: &'static Scenario,
    pub run: RunId,
    pub cwd: &'a std::path::Path,
    pub session: Option<AgentSessionId>,
    pub resume: bool,
    pub bin: PathBuf,
}

/// How long [`replay`] waits for every replayer to finish once the driver has stopped what it ran, which each does
/// as soon as the adapter closes the CLI's stdin, unless one has gone wrong.
const FINISHING: Duration = Duration::from_secs(10);

/// Replays `fixture`, a capture of `scenario` on the scripted model, through the adapter's own process code, and runs
/// the scenario's checks on what the adapter made of it.
pub async fn replay(binding: &'static dyn Binding, scenario: &'static Scenario, fixture: &str) -> Report {
    let started = Instant::now();
    let report = |trial, steps, problems: Vec<String>| {
        let wrote = if problems.is_empty() { Ok(()) } else { Err(problems.join("; ")) };
        let also = vec![("the adapter did what it did when the trial was captured", wrote)];
        Report::of(scenario, Played::Replayed, &trial, steps, also, started)
    };
    let dir = match tempfile::Builder::new().prefix("agent-replay-").tempdir() {
        Ok(dir) => dir,
        Err(e) => return report(Trial::default(), Err(Failed(format!("making a directory: {e}"))), Vec::new()),
    };
    let processes = match capture::processes(&fixture.replace("/scratch", &dir.path().display().to_string())) {
        Ok(processes) => processes,
        Err(e) => return report(Trial::default(), Err(Failed(format!("reading the fixture: {e}"))), Vec::new()),
    };
    let (saw, seen) = mpsc::unbounded_channel();
    let state = Arc::new(Mutex::new(Replayed { processes: processes.into(), stand_ins: Vec::new() }));
    let launcher = StandIns { binding, scenario, dir: dir.path().to_path_buf(), state: state.clone(), saw, seen };
    let mut driver = Driver::new(Box::new(launcher));
    let steps = (scenario.steps)(&mut driver).await;
    let (trial, stopped) = driver.finish().await;
    let Replayed { processes, stand_ins } = std::mem::take(&mut *state.lock());
    let finished = tokio::task::spawn_blocking(move || stand_ins.into_iter().flat_map(StandIn::finish).collect());
    let mut problems: Vec<String> = match tokio::time::timeout(FINISHING, finished).await {
        Ok(Ok(problems)) => problems,
        Ok(Err(_)) => vec!["a replayer panicked".into()],
        Err(_) => vec!["a replayer didn't finish once its stand-in had gone".into()],
    };
    if !processes.is_empty() {
        problems.push(format!("the steps launched {} fewer processes than the capture has", processes.len()));
    }
    report(trial, steps.and(stopped), problems)
}

/// Launches a stand-in for the CLI for each process of a capture in turn.
struct StandIns {
    binding: &'static dyn Binding,
    scenario: &'static Scenario,
    dir: PathBuf,
    /// Shared with [`replay`], which says what's left of the capture once the steps are done.
    state: Arc<Mutex<Replayed>>,
    /// Where each stand-in says it has reached what a step saw.
    saw: mpsc::UnboundedSender<String>,
    seen: mpsc::UnboundedReceiver<String>,
}

#[derive(Default)]
struct Replayed {
    /// Those not launched yet.
    processes: VecDeque<Vec<Entry>>,
    /// Those launched, in order, each to say what the adapter did that the capture didn't have.
    stand_ins: Vec<StandIn>,
}

#[async_trait]
impl Launcher for StandIns {
    async fn launch(
        &mut self,
        run: RunId,
        resume: bool,
        output: mpsc::Sender<AgentOutput>,
    ) -> Result<Arc<dyn AgentProcess>, Failed> {
        let next = self.state.lock().processes.pop_front();
        let process = next.ok_or_else(|| Failed("the capture has no more processes".into()))?;
        let Some(Entry::Launch { resume: captured, session, .. }) = process.first().cloned() else {
            return Err(Failed("a process in the capture doesn't start with its launch".into()));
        };
        if captured != resume {
            return Err(Failed(format!("the step launched with resume {resume}, and the capture with {captured}")));
        }
        let dir = self.dir.join(format!("process-{}", run.get()));
        let stand_in = std::fs::create_dir(&dir)
            .and_then(|()| StandIn::start(&dir, process, self.binding.lines(), self.saw.clone()))
            .map_err(|e| Failed(format!("starting the stand-in: {e}")))?;
        let launch =
            Launch { scenario: self.scenario, run, cwd: &self.dir, session, resume, bin: stand_in.program.clone() };
        let spawned = self.binding.spawn(launch, output).await;
        // Kept even when the adapter couldn't start it, so its replayer is let go of and says so.
        self.state.lock().stand_ins.push(stand_in);
        spawned.map_err(|e| Failed(format!("launching the stand-in: {e}")))
    }

    async fn made(&mut self, name: &str) -> Result<(), Failed> {
        let waiting = async {
            while let Some(saw) = self.seen.recv().await {
                if saw == name {
                    return true;
                }
            }
            false
        };
        if tokio::time::timeout(QUIET, waiting).await.unwrap_or(false) {
            Ok(())
        } else {
            Err(Failed(format!("the capture never saw {name} where the steps looked for it")))
        }
    }

    fn patience(&self) -> Duration {
        QUIET
    }
}
