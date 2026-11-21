//! Playing a scenario on Codex's or Antigravity's real CLI, through the adapter, capturing what crosses its stdio.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::Mutex;
use protocol::capability::conversation::Start;
use protocol::event::EventBody;
use protocol::model::{AgentSessionId, RunId};
use tokio::sync::mpsc;

use super::agent::{JsonRpcAgent, spec};
use super::lines::JsonRpc;
use crate::conformance::capture::{self, Entry, Tap};
use crate::conformance::driver::{Driver, Launcher};
use crate::conformance::scenario::{Against, Failed, Scenario};
use crate::conformance::trial::{Live, Played, Report, Trial};
use crate::{AgentOutput, AgentProcess};

/// What a live trial uses.
pub struct Options {
    pub cli: PathBuf,
    /// This machine's home, which a scripted trial's Codex can't read, and where a trial of Antigravity on the real
    /// model leaves the files of its conversation, which are removed once it ends. `None` leaves both alone.
    pub home: Option<PathBuf>,
}

/// Plays `scenario` against `against` on `agent`'s real CLI, through its adapter.
pub(crate) async fn live(
    agent: &'static dyn JsonRpcAgent,
    scenario: &'static Scenario,
    against: Against,
    options: &Options,
) -> Live {
    let started = Instant::now();
    match play(agent, scenario, against, options, started).await {
        Ok(live) => live,
        Err(why) => Live {
            report: Report::of(
                scenario,
                Played::Live(against),
                &Trial::default(),
                Err(Failed(why)),
                Vec::new(),
                started,
            ),
            fixture: None,
            requests: Vec::new(),
        },
    }
}

async fn play(
    agent: &'static dyn JsonRpcAgent,
    scenario: &'static Scenario,
    against: Against,
    options: &Options,
    started: Instant,
) -> Result<Live, String> {
    let prefix = format!("{}-conformance-", agent.name().to_lowercase());
    let room = tempfile::Builder::new().prefix(&prefix).tempdir().map_err(|e| e.to_string())?;
    let work = room.path().join("work");
    std::fs::create_dir(&work).map_err(|e| e.to_string())?;
    std::fs::write(work.join("README.md"), "A scratch project.\n").map_err(|e| e.to_string())?;
    let model = if against == Against::ScriptedModel {
        Some(agent.scripted_model(scenario).await.map_err(|e| e.to_string())?)
    } else {
        None
    };
    let (bin, env) = agent.prepare(room.path(), options, model.as_ref()).map_err(|e| e.to_string())?;
    let (tap, mut captured) = Tap::new();
    let conversation = Arc::new(Mutex::new(None));
    let launcher =
        RealCli { agent, bin, work: work.clone(), env, scenario, against, conversation: conversation.clone(), tap };
    let mut driver = Driver::new(Box::new(launcher));
    let steps = (scenario.steps)(&mut driver).await;
    let (trial, stopped) = driver.finish().await;
    let left = (against == Against::RealModel).then(|| conversation.lock().clone()).flatten();
    let forgotten = match left {
        Some(id) => agent.forget(options, &work, &id).await,
        None => Ok(()),
    };
    let mut entries = Vec::new();
    while let Ok(mut entry) = captured.try_recv() {
        agent.scrub(&mut entry);
        entries.push(entry);
    }
    let report =
        Report::of(scenario, Played::Live(against), &trial, steps.and(stopped).and(forgotten), Vec::new(), started);
    let fixture = (against == Against::ScriptedModel)
        .then(|| capture::fixture(&entries, &agent.named(room.path()), JsonRpc::of(agent.dialect())));
    Ok(Live { report, fixture, requests: model.map_or_default(|model| model.requests()) })
}

/// Launches the agent's real CLI for each of a trial's processes, keeping the conversation its first one reports for
/// a step that resumes it.
struct RealCli {
    agent: &'static dyn JsonRpcAgent,
    bin: PathBuf,
    work: PathBuf,
    env: Vec<(String, String)>,
    scenario: &'static Scenario,
    against: Against,
    conversation: Arc<Mutex<Option<AgentSessionId>>>,
    tap: Tap,
}

#[async_trait]
impl Launcher for RealCli {
    async fn launch(
        &mut self,
        run: RunId,
        resume: bool,
        output: mpsc::Sender<AgentOutput>,
    ) -> Result<Arc<dyn AgentProcess>, Failed> {
        let start = if resume {
            let missing = || Failed(format!("{} reported no conversation", self.agent.name()));
            Start::Resume { conversation: self.conversation.lock().clone().ok_or_else(missing)? }
        } else {
            Start::New { conversation: None }
        };
        let spec = spec(self.agent, self.scenario, run, &self.work, start, self.env.clone())?;
        let (sender, mut receiver) = mpsc::channel(1024);
        let conversation = self.conversation.clone();
        tokio::spawn(async move {
            while let Some(item) = receiver.recv().await {
                if let AgentOutput::Event { body: EventBody::AgentInit { agent_session_id, .. }, .. } = &item {
                    (*conversation.lock()).clone_from(agent_session_id);
                }
                if output.send(item).await.is_err() {
                    break;
                }
            }
        });
        self.agent
            .start(self.bin.clone(), spec, sender, Some(self.tap.clone()))
            .await
            .map_err(|e| Failed(e.to_string()))
    }

    async fn made(&mut self, name: &str) -> Result<(), Failed> {
        if !waiting::task::up_to(self.patience(), || self.work.join(name).exists()).await {
            return Err(Failed(format!("{} did not make {name}", self.agent.name())));
        }
        self.tap.note(Entry::Saw(name.into()));
        Ok(())
    }

    fn patience(&self) -> Duration {
        Duration::from_secs(if self.against == Against::ScriptedModel { 30 } else { 180 })
    }
}
