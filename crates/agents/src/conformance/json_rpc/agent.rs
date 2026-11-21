use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use protocol::capability::conversation::Start;
use protocol::capability::settings::Effort;
use protocol::model::{AgentDescription, AgentSessionId, InteractionMode, ModelName, RunId};
use protocol::worker::AgentSpec;
use tokio::sync::mpsc;

use super::live::Options;
use crate::conformance::capture::{Entry, Tap};
use crate::conformance::http::Server;
use crate::conformance::scenario::{Failed, Scenario};
use crate::jsonrpc::Dialect;
use crate::{AgentError, AgentOutput, AgentProcess};

/// What playing a scenario on Codex or Antigravity needs that's the agent's own. Playing it, live or replayed, is the
/// same for both.
#[async_trait]
pub(crate) trait JsonRpcAgent: Send + Sync {
    /// The agent's name, as a failure says it.
    fn name(&self) -> &'static str;
    fn dialect(&self) -> Dialect;
    /// The agent's description, which says its kind and the modes a scenario's chat runs in.
    fn description(&self) -> AgentDescription;
    /// The model a trial runs, at the least effort.
    fn model(&self) -> ModelName;

    /// Starts the adapter on `bin`, telling `tap`, if there's one, what crosses the CLI's stdio.
    async fn start(
        &self,
        bin: PathBuf,
        spec: AgentSpec,
        output: mpsc::Sender<AgentOutput>,
        tap: Option<Tap>,
    ) -> Result<Arc<dyn AgentProcess>, AgentError>;

    /// Starts the scripted model a trial plays `scenario` on.
    async fn scripted_model(&self, scenario: &'static Scenario) -> std::io::Result<Server>;

    /// The CLI a live trial in `room` launches, and the environment it's launched with, against `model` if it's the
    /// scripted one.
    fn prepare(
        &self,
        room: &Path,
        options: &Options,
        model: Option<&Server>,
    ) -> std::io::Result<(PathBuf, Vec<(String, String)>)>;

    /// What a fixture calls `room`, as it's given and as it resolves, in the order a fixture's strings are replaced.
    fn named(&self, room: &Path) -> Vec<(String, String)>;

    /// Takes out of a captured entry what differs from one trial to the next.
    fn scrub(&self, _entry: &mut Entry) {}

    /// Removes `id`, the conversation a trial on the real model left on this machine, played in `work`.
    async fn forget(&self, options: &Options, work: &Path, id: &AgentSessionId) -> Result<(), Failed>;
}

/// What `agent`'s process for `run` is launched with in `cwd`: `start`, in the scenario's mode, on the agent's trial
/// model at the least effort.
pub(super) fn spec(
    agent: &dyn JsonRpcAgent,
    scenario: &Scenario,
    run: RunId,
    cwd: &Path,
    start: Start,
    env: Vec<(String, String)>,
) -> Result<AgentSpec, Failed> {
    let description = agent.description();
    let settings = scenario.settings(&description, agent.model(), Effort::Low)?;
    Ok(AgentSpec {
        session_id: "conformance".into(),
        run,
        agent: description.kind,
        cwd: cwd.display().to_string(),
        start,
        settings,
        env,
        tools_token: None,
        login: None,
        repository: None,
        interaction: InteractionMode::Typing,
    })
}
