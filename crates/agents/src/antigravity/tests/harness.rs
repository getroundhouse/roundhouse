#![cfg(test)]
use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use protocol::capability::approvals::ApprovalMode;
use protocol::capability::conversation::Start;
use protocol::event::EventBody;
use protocol::model::{AgentKind, ModelName, RunId};
use protocol::worker::AgentSpec;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::antigravity::launch::Purpose;
use crate::antigravity::running::{AntigravityProcess, start_conversation};
use crate::{AgentError, AgentOptions, AgentOutput, BinaryLookup, ToolServer};

pub(super) static PROGRAM: process::testing::Example = process::testing::Example::new("native_standin");

pub(super) struct Harness {
    /// The agent's working directory, and its home, where the stand-in records what it was asked.
    pub(super) room: tempfile::TempDir,
    pub(super) rx: mpsc::Receiver<AgentOutput>,
    pub(super) launch: JoinHandle<Result<Arc<AntigravityProcess>, AgentError>>,
}

impl Harness {
    pub(super) fn new(rules: &Value, mode: ApprovalMode, start: Start) -> Self {
        Self::with_tools(rules, mode, start, None)
    }

    pub(super) fn with_tools(rules: &Value, mode: ApprovalMode, start: Start, tools: Option<ToolServer>) -> Self {
        Self::launching(rules, mode, start, tools, |_| {}, None)
    }

    /// A new conversation in full access on `model` rather than the kind's default.
    pub(super) fn on_model(rules: &Value, model: &'static str) -> Self {
        let start = Start::New { conversation: None };
        Self::launching(rules, super::super::claims::full_access(), start, None, |_| {}, Some(ModelName::named(model)))
    }

    /// One whose home already has `conversation`'s own file, as `agy` keeps it, before the launch.
    pub(super) fn keeping(rules: &Value, mode: ApprovalMode, start: Start, conversation: &str) -> Self {
        Self::seeded(rules, mode, start, |room| {
            let kept = room.join(".gemini").join("antigravity-cli").join("conversations");
            std::fs::create_dir_all(&kept).unwrap();
            std::fs::write(kept.join(format!("{conversation}.db")), b"kept").unwrap();
        })
    }

    pub(super) fn seeded(rules: &Value, mode: ApprovalMode, start: Start, seed: impl FnOnce(&std::path::Path)) -> Self {
        Self::launching(rules, mode, start, None, seed, None)
    }

    fn launching(
        rules: &Value,
        mode: ApprovalMode,
        start: Start,
        tools: Option<ToolServer>,
        seed: impl FnOnce(&std::path::Path),
        model: Option<ModelName>,
    ) -> Self {
        let room = tempfile::tempdir().unwrap();
        seed(room.path());
        let bin = PROGRAM.path();
        std::fs::write(room.path().join("native-script.json"), serde_json::to_vec(rules).unwrap()).unwrap();
        let mut settings = crate::antigravity::description().defaults();
        settings.mode = Some(mode);
        if let Some(model) = model {
            settings.model = Some(model);
        }
        let spec = AgentSpec {
            session_id: "test".into(),
            run: RunId::new(1),
            agent: AgentKind::Antigravity,
            cwd: room.path().to_string_lossy().into_owned(),
            start,
            settings,
            env: Vec::new(),
            tools_token: None,
            login: None,
            repository: None,
            interaction: protocol::model::InteractionMode::Typing,
        };
        let options = AgentOptions {
            // `agy` keeps its conversations under the home, which the room stands in for.
            binary: BinaryLookup { home: Some(room.path().to_owned()), ..BinaryLookup::default() },
            pid_file: Some(room.path().join("pid")),
            tools,
            instructions: Some(crate::Instructions {
                text: "Roundhouse instructions".into(),
                file: room.path().join("instructions.md"),
            }),
            ..Default::default()
        };
        let (tx, rx) = mpsc::channel(1024);
        let launch =
            tokio::spawn(async move { start_conversation(&bin, &spec, tx, &options, Purpose::Agent, None).await });
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

    pub(super) async fn reply_to(&mut self, id: &Value) -> Value {
        loop {
            if let AgentOutput::Event { body: EventBody::AgentOther { kind, .. }, raw: Some(raw), .. } =
                self.next().await
                && kind == "test/request"
            {
                let value: Value = serde_json::from_str(&raw).unwrap();
                if &value["params"]["id"] == id && value["params"].get("method").is_none() {
                    return value["params"].clone();
                }
            }
        }
    }

    pub(super) async fn next(&mut self) -> AgentOutput {
        tokio::time::timeout(Duration::from_secs(10), self.rx.recv()).await.unwrap().unwrap()
    }

    pub(super) async fn started(&mut self) -> Arc<AntigravityProcess> {
        tokio::time::timeout(Duration::from_secs(10), &mut self.launch).await.unwrap().unwrap().unwrap()
    }

    /// Why the launch failed, once it has.
    pub(super) async fn refused(&mut self) -> AgentError {
        let launched = tokio::time::timeout(Duration::from_secs(10), &mut self.launch).await.unwrap().unwrap();
        launched.err().expect("the launch failed")
    }
}

pub(super) fn update(payload: &Value) -> Value {
    json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": payload}})
}

pub(super) fn user_chunk(text: &str) -> Value {
    update(&json!({"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": text}}))
}

pub(super) fn thought_chunk(text: &str) -> Value {
    update(&json!({"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": text}}))
}

pub(super) fn msg_chunk(text: &str) -> Value {
    update(&json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}))
}

pub(super) fn append_transcript_raw(room: &Path, raw: &str) {
    let dir =
        room.join(".gemini").join("antigravity-cli").join("brain").join("session-1").join(".system_generated/logs");
    std::fs::create_dir_all(&dir).unwrap();
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("transcript.jsonl")).unwrap();
    file.write_all(raw.as_bytes()).unwrap();
    file.flush().unwrap();
}

pub(super) fn append_transcript(room: &Path, steps: &[Value]) {
    let mut raw = String::new();
    for step in steps {
        raw.push_str(&step.to_string());
        raw.push('\n');
    }
    append_transcript_raw(room, &raw);
}
