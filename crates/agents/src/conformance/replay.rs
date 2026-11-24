//! Replaying a capture of a scenario through the adapter's own process code, with a stand-in for the CLI.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use protocol::model::{AgentSessionId, RunId};
use tokio::sync::mpsc;

use crate::conformance::capture::{self, Entry};
use crate::conformance::driver::{Driver, Launcher};
use crate::conformance::lines::Lines;
use crate::conformance::quoted;
use crate::conformance::scenario::{Failed, Scenario};
use crate::conformance::stand_in::{QUIET, StandIn};
use crate::conformance::trial::{Played, Report, Trial};
use crate::{AgentOutput, AgentProcess};

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
    pub cwd: &'a Path,
    pub session: Option<AgentSessionId>,
    pub resume: bool,
    pub bin: PathBuf,
}

/// How long [`replay`] waits for every replayer to finish once the driver has stopped what it ran, which each does
/// as soon as the adapter closes the CLI's stdin, unless one has gone wrong.
const FINISHING: Duration = Duration::from_secs(10);

const DISPATCHER: &str = r#"n=0
while ! mkdir "$DIR/claimed-$n" 2>/dev/null; do
  n=$((n + 1))
done
if [ ! -x "$DIR/proc-$n/stand-in" ]; then
  echo "the adapter launched more CLI processes than the capture has" >&2
  exit 99
fi
exec "$DIR/proc-$n/stand-in" "$@"
"#;

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
    let (bin, meta, stand_ins) = match prepare_stand_ins(dir.path(), processes, binding.lines(), &saw) {
        Ok(prepared) => prepared,
        Err(e) => return report(Trial::default(), Err(e), Vec::new()),
    };
    let total = meta.len();
    let launcher = StandIns { binding, scenario, dir: dir.path().to_path_buf(), bin, meta, seen };
    let mut driver = Driver::new(Box::new(launcher));
    let steps = (scenario.steps)(&mut driver).await;
    let (trial, stopped) = driver.finish().await;
    let unclaimed = (0..total).filter(|idx| !dir.path().join(format!("claimed-{idx}")).exists()).count();
    let finished = tokio::task::spawn_blocking(move || stand_ins.into_iter().flat_map(StandIn::finish).collect());
    let mut problems: Vec<String> = match tokio::time::timeout(FINISHING, finished).await {
        Ok(Ok(problems)) => problems,
        Ok(Err(_)) => vec!["a replayer panicked".into()],
        Err(_) => vec!["a replayer didn't finish once its stand-in had gone".into()],
    };
    if unclaimed > 0 {
        problems.push(format!("the steps launched {unclaimed} fewer processes than the capture has"));
    }
    report(trial, steps.and(stopped), problems)
}

type Prepared = (PathBuf, Vec<(bool, Option<AgentSessionId>)>, Vec<StandIn>);

fn prepare_stand_ins(
    root: &Path,
    processes: Vec<Vec<Entry>>,
    lines: &'static dyn Lines,
    saw: &mpsc::UnboundedSender<String>,
) -> Result<Prepared, Failed> {
    let script = format!("#!/bin/sh\nDIR={}\n{DISPATCHER}", quoted(root));
    let bin = root.join("stand-in");
    process::testing::write_program(&bin, script).map_err(|e| Failed(format!("making dispatcher: {e}")))?;
    let mut meta = Vec::with_capacity(processes.len());
    let mut stand_ins = Vec::with_capacity(processes.len());
    for (idx, process) in processes.into_iter().enumerate() {
        let Some(Entry::Launch { resume, session, .. }) = process.first().cloned() else {
            return Err(Failed("a process in the capture doesn't start with its launch".into()));
        };
        let proc_dir = root.join(format!("proc-{idx}"));
        let stand_in = std::fs::create_dir(&proc_dir)
            .and_then(|()| StandIn::start(&proc_dir, process, lines, saw.clone()))
            .map_err(|e| Failed(format!("starting the stand-in: {e}")))?;
        meta.push((resume, session));
        stand_ins.push(stand_in);
    }
    Ok((bin, meta, stand_ins))
}

/// Launches a stand-in for the CLI for each process of a capture in turn, including internal adapter restarts.
struct StandIns {
    binding: &'static dyn Binding,
    scenario: &'static Scenario,
    dir: PathBuf,
    bin: PathBuf,
    meta: Vec<(bool, Option<AgentSessionId>)>,
    seen: mpsc::UnboundedReceiver<String>,
}

#[async_trait]
impl Launcher for StandIns {
    async fn launch(
        &mut self,
        run: RunId,
        resume: bool,
        output: mpsc::Sender<AgentOutput>,
    ) -> Result<Arc<dyn AgentProcess>, Failed> {
        let next = (0..self.meta.len()).find(|idx| !self.dir.join(format!("claimed-{idx}")).exists());
        let idx = next.ok_or_else(|| Failed("the capture has no more processes".into()))?;
        let (captured, ref session) = self.meta[idx];
        if captured != resume {
            return Err(Failed(format!("the step launched with resume {resume}, and the capture with {captured}")));
        }
        let launch = Launch {
            scenario: self.scenario,
            run,
            cwd: &self.dir,
            session: session.clone(),
            resume,
            bin: self.bin.clone(),
        };
        self.binding.spawn(launch, output).await.map_err(|e| Failed(format!("launching the stand-in: {e}")))
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
