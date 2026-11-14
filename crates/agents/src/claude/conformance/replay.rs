//! Claude's launch for the shared conformance replayer.
use super::stream_json::StreamJson;
use crate::conformance::{
    lines::Lines,
    replay::{self, Binding, Launch},
    scenario::{Failed, Scenario},
    trial::Report,
};
use crate::{AgentOptions, AgentOutput, AgentProcess};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

struct Claude;
#[async_trait]
impl Binding for Claude {
    fn lines(&self) -> &'static dyn Lines {
        &StreamJson
    }
    async fn spawn(
        &self,
        launch: Launch<'_>,
        output: mpsc::Sender<AgentOutput>,
    ) -> Result<Arc<dyn AgentProcess>, Failed> {
        let session = launch.session.ok_or_else(|| Failed("Claude capture has no conversation ID".into()))?;
        let spec = super::spec(launch.scenario, launch.run, launch.cwd, &session, launch.resume, Vec::new())?;
        crate::claude::spawn_at(launch.bin, spec, output, AgentOptions::default())
            .await
            .map_err(|e| Failed(e.to_string()))
    }
}

pub async fn replay(scenario: &'static Scenario, fixture: &str) -> Report {
    replay::replay(&Claude, scenario, fixture).await
}
