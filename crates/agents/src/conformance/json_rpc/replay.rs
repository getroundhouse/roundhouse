//! Replaying a capture of Codex or Antigravity with the shared replayer.

use std::sync::Arc;

use async_trait::async_trait;
use protocol::capability::conversation::Start;
use tokio::sync::mpsc;

use super::agent::{JsonRpcAgent, spec};
use super::lines::JsonRpc;
use crate::conformance::lines::Lines;
use crate::conformance::replay::{Binding, Launch};
use crate::conformance::scenario::Failed;
use crate::{AgentOutput, AgentProcess};

#[async_trait]
impl<A: JsonRpcAgent> Binding for A {
    fn lines(&self) -> &'static dyn Lines {
        JsonRpc::of(self.dialect())
    }

    async fn spawn(
        &self,
        launch: Launch<'_>,
        output: mpsc::Sender<AgentOutput>,
    ) -> Result<Arc<dyn AgentProcess>, Failed> {
        let cwd = launch.cwd.join("work");
        std::fs::create_dir_all(&cwd).map_err(|e| Failed(e.to_string()))?;
        let start = if launch.resume {
            let missing = || Failed(format!("{} resume capture has no conversation ID", self.name()));
            Start::Resume { conversation: launch.session.ok_or_else(missing)? }
        } else {
            Start::New { conversation: None }
        };
        let spec = spec(self, launch.scenario, launch.run, &cwd, start, Vec::new())?;
        self.start(launch.bin, spec, output, None).await.map_err(|e| Failed(e.to_string()))
    }
}
