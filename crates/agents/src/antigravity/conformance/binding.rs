//! What playing a scenario on Antigravity needs that's its own: its scripted model, the home a scripted trial's CLI
//! runs in, the times its output carries, and removing the files of the conversation a trial on the real model made.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use protocol::model::{AgentDescription, AgentSessionId, ModelName};
use protocol::worker::AgentSpec;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::conformance::capture::{Entry, Tap};
use crate::conformance::http::Server;
use crate::conformance::json_rpc::{self, JsonRpcAgent, Options};
use crate::conformance::replay as replayer;
use crate::conformance::scenario::{Against, Failed, Scenario};
use crate::conformance::trial::{Live, Report};
use crate::jsonrpc::Dialect;
use crate::{AgentError, AgentOptions, AgentOutput, AgentProcess, BinaryLookup};

struct Antigravity;

pub fn find_cli(lookup: &BinaryLookup) -> Option<PathBuf> {
    crate::binary::resolve_binary("agy", &[], lookup)
}

pub async fn live(scenario: &'static Scenario, against: Against, options: &Options) -> Live {
    json_rpc::live(&Antigravity, scenario, against, options).await
}

pub async fn replay(scenario: &'static Scenario, fixture: &str) -> Report {
    replayer::replay(&Antigravity, scenario, fixture).await
}

#[async_trait]
impl JsonRpcAgent for Antigravity {
    fn name(&self) -> &'static str {
        "Antigravity"
    }
    fn dialect(&self) -> Dialect {
        Dialect::JsonRpc2
    }
    fn description(&self) -> AgentDescription {
        crate::antigravity::description()
    }
    fn model(&self) -> ModelName {
        ModelName::named(super::super::description::TRIAL_MODEL)
    }

    async fn start(
        &self,
        bin: PathBuf,
        spec: AgentSpec,
        output: mpsc::Sender<AgentOutput>,
        tap: Option<Tap>,
    ) -> Result<Arc<dyn AgentProcess>, AgentError> {
        match tap {
            Some(tap) => crate::antigravity::spawn_tapped(bin, spec, output, AgentOptions::default(), tap).await,
            None => crate::antigravity::spawn_at(bin, spec, output, AgentOptions::default()).await,
        }
    }

    async fn scripted_model(&self, scenario: &'static Scenario) -> std::io::Result<Server> {
        super::gemini::start(scenario).await
    }

    fn prepare(
        &self,
        room: &Path,
        options: &Options,
        model: Option<&Server>,
    ) -> std::io::Result<(PathBuf, Vec<(String, String)>)> {
        let Some(model) = model else { return Ok((options.cli.clone(), Vec::new())) };
        let home = room.join("home");
        let config_dir = home.join(".gemini").join("antigravity-cli");
        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(config_dir.join("settings.json"), json!({ "modelProvider": "gemini" }).to_string())?;
        Ok((
            options.cli.clone(),
            vec![
                ("HOME".into(), home.display().to_string()),
                ("GEMINI_API_KEY".into(), "scripted".into()),
                ("GOOGLE_GEMINI_BASE_URL".into(), model.url().into()),
                ("HTTPS_PROXY".into(), "http://127.0.0.1:9".into()),
                ("HTTP_PROXY".into(), "http://127.0.0.1:9".into()),
                ("NO_PROXY".into(), "127.0.0.1,localhost".into()),
            ],
        ))
    }

    fn named(&self, room: &Path) -> Vec<(String, String)> {
        let mut named = Vec::new();
        if let Ok(resolved) = room.canonicalize() {
            named.push((resolved.display().to_string(), "/scratch".into()));
        }
        named.push((room.display().to_string(), "/scratch".into()));
        named
    }

    fn scrub(&self, entry: &mut Entry) {
        if let Entry::FromCli(value) = entry {
            scrub_times(value);
        }
    }

    async fn forget(&self, options: &Options, _work: &Path, id: &AgentSessionId) -> Result<(), Failed> {
        if let Some(files) =
            options.home.as_deref().and_then(|home| super::super::files::ConversationFiles::of(home, id))
        {
            files.remove().await;
        }
        Ok(())
    }
}

/// `value` with the time in each of its `Created At: ` and `Completed At: ` lines made the same in every trial.
fn scrub_times(value: &mut Value) {
    match value {
        Value::String(s) if s.contains("Created At: ") || s.contains("Completed At: ") => {
            *s = s
                .lines()
                .map(|line| {
                    if line.starts_with("Created At: ") {
                        "Created At: 1970-01-01T00:00:00Z"
                    } else if line.starts_with("Completed At: ") {
                        "Completed At: 1970-01-01T00:00:00Z"
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
                + if s.ends_with('\n') { "\n" } else { "" };
        }
        Value::Array(items) => items.iter_mut().for_each(scrub_times),
        Value::Object(map) => map.values_mut().for_each(scrub_times),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}
