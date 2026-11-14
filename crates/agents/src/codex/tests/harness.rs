#![cfg(test)]
use std::sync::Arc;
use std::time::Duration;

use protocol::capability::conversation::Start;
use protocol::event::EventBody;
use protocol::model::{AgentKind, RunId};
use protocol::worker::AgentSpec;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::codex::running::{CodexProcess, start_conversation};
use crate::{AgentOptions, AgentOutput};

pub(super) static PROGRAM: crate::testing::Example = crate::testing::Example::new("native_standin");

pub(super) struct Harness {
    pub(super) room: tempfile::TempDir,
    pub(super) rx: mpsc::Receiver<AgentOutput>,
    pub(super) launch: JoinHandle<Arc<CodexProcess>>,
}

impl Harness {
    pub(super) fn new(rules: &Value) -> Self {
        let room = tempfile::tempdir().unwrap();
        let bin = PROGRAM.path();
        std::fs::write(room.path().join("native-script.json"), serde_json::to_vec(rules).unwrap()).unwrap();
        let mut settings = crate::codex::description().defaults;
        settings.model = protocol::model::ModelName::named("gpt-6-luna");
        let spec = AgentSpec {
            session_id: "test".into(),
            run: RunId::new(1),
            agent: AgentKind::Codex,
            cwd: room.path().to_string_lossy().into_owned(),
            start: Start::New { conversation: None },
            settings,
            env: Vec::new(),
            tools_token: None,
            login: None,
        };
        let options = AgentOptions { pid_file: Some(room.path().join("pid")), ..Default::default() };
        let (tx, rx) = mpsc::channel(1024);
        let launch =
            tokio::spawn(async move { start_conversation(&bin, &spec, tx, &options, false, None).await.unwrap() });
        Self { room, rx, launch }
    }

    pub(super) async fn request(&mut self, method: &str) -> Value {
        loop {
            if let AgentOutput::Event { body: EventBody::AgentOther { kind, .. }, raw: Some(raw), .. } =
                self.next().await
                && kind == "test/request"
            {
                let value: Value = serde_json::from_str(&raw).unwrap();
                if value["params"]["method"] == method {
                    return value["params"].clone();
                }
            }
        }
    }

    pub(super) async fn next(&mut self) -> AgentOutput {
        tokio::time::timeout(Duration::from_secs(10), self.rx.recv()).await.unwrap().unwrap()
    }

    pub(super) async fn started(&mut self) -> Arc<CodexProcess> {
        tokio::time::timeout(Duration::from_secs(10), &mut self.launch).await.unwrap().unwrap()
    }
}
