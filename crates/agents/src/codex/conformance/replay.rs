//! Codex's launch for the shared conformance replayer.
use super::wire::JsonRpc;
use crate::conformance::{
    lines::Lines,
    replay::{self, Binding, Launch},
    scenario::{Failed, Scenario},
    trial::Report,
};
use crate::{AgentOptions, AgentOutput, AgentProcess};
use async_trait::async_trait;
use protocol::{capability::conversation::Start, model::AgentKind, worker::AgentSpec};
use std::sync::Arc;
use tokio::sync::mpsc;

struct Codex;
#[async_trait]
impl Binding for Codex {
    fn lines(&self) -> &'static dyn Lines {
        &JsonRpc
    }
    async fn spawn(
        &self,
        launch: Launch<'_>,
        output: mpsc::Sender<AgentOutput>,
    ) -> Result<Arc<dyn AgentProcess>, Failed> {
        let cwd = launch.cwd.join("work");
        std::fs::create_dir_all(&cwd).map_err(|e| Failed(e.to_string()))?;
        let start = if launch.resume {
            Start::Resume {
                conversation: launch
                    .session
                    .ok_or_else(|| Failed("Codex resume capture has no conversation ID".into()))?,
            }
        } else {
            Start::New { conversation: None }
        };
        let luna = protocol::model::ModelName::named("gpt-6-luna");
        let low = protocol::capability::settings::Effort::Low;
        let settings = launch.scenario.settings(&crate::codex::description(), luna, low)?;
        let spec = AgentSpec {
            session_id: "conformance".into(),
            run: launch.run,
            agent: AgentKind::Codex,
            cwd: cwd.display().to_string(),
            start,
            settings,
            env: Vec::new(),
            tools_token: None,
            login: None,
            repository: None,
            interaction: protocol::model::InteractionMode::Typing,
        };
        crate::codex::spawn_at(launch.bin, spec, output, AgentOptions::default())
            .await
            .map_err(|e| Failed(e.to_string()))
    }
}

pub async fn replay(scenario: &'static Scenario, fixture: &str) -> Report {
    replay::replay(&Codex, scenario, fixture).await
}
