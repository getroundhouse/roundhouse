//! Antigravity's launch for the shared conformance replayer.

use std::sync::Arc;

use async_trait::async_trait;
use protocol::{
    capability::conversation::Start,
    capability::settings::Effort,
    model::{AgentKind, ModelName},
    worker::AgentSpec,
};
use tokio::sync::mpsc;

use super::wire::JsonRpc;
use crate::conformance::{
    lines::Lines,
    replay::{self, Binding, Launch},
    scenario::{Failed, Scenario},
    trial::Report,
};
use crate::{AgentOptions, AgentOutput, AgentProcess};

struct Antigravity;

#[async_trait]
impl Binding for Antigravity {
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
                    .ok_or_else(|| Failed("Antigravity resume capture has no conversation ID".into()))?,
            }
        } else {
            Start::New { conversation: None }
        };
        let trial_model = ModelName::named(super::super::description::TRIAL_MODEL);
        let settings = launch.scenario.settings(&crate::antigravity::description(), trial_model, Effort::Low)?;
        let spec = AgentSpec {
            session_id: "conformance".into(),
            run: launch.run,
            agent: AgentKind::Antigravity,
            cwd: cwd.display().to_string(),
            start,
            settings,
            env: Vec::new(),
            tools_token: None,
            login: None,
            repository: None,
        };
        crate::antigravity::spawn_at(launch.bin, spec, output, AgentOptions::default())
            .await
            .map_err(|e| Failed(e.to_string()))
    }
}

pub async fn replay(scenario: &'static Scenario, fixture: &str) -> Report {
    replay::replay(&Antigravity, scenario, fixture).await
}
