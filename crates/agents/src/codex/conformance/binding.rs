//! What playing a scenario on Codex needs that's its own: its scripted model, the sandbox a scripted trial's CLI runs
//! in, and deleting the thread a trial on the real model made.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use protocol::model::{AgentDescription, AgentKind, AgentSessionId, ModelName};
use protocol::worker::AgentSpec;
use serde_json::json;
use tokio::sync::mpsc;

use crate::conformance::capture::Tap;
use crate::conformance::http::Server;
use crate::conformance::json_rpc::{self, JsonRpcAgent, Options};
use crate::conformance::scenario::{Against, Failed, Scenario};
use crate::conformance::trial::{Live, Report};
use crate::conformance::{executable, quoted, replay as replayer};
use crate::jsonrpc::Dialect;
use crate::{AgentError, AgentOptions, AgentOutput, AgentProcess, BinaryLookup};

struct Codex;

pub fn find_cli(lookup: &BinaryLookup) -> Option<PathBuf> {
    crate::binary::resolve_binary("codex", &[], lookup)
}

pub async fn live(scenario: &'static Scenario, against: Against, options: &Options) -> Live {
    json_rpc::live(&Codex, scenario, against, options).await
}

pub async fn replay(scenario: &'static Scenario, fixture: &str) -> Report {
    replayer::replay(&Codex, scenario, fixture).await
}

#[async_trait]
impl JsonRpcAgent for Codex {
    fn name(&self) -> &'static str {
        "Codex"
    }
    fn dialect(&self) -> Dialect {
        Dialect::Bare
    }
    fn description(&self) -> AgentDescription {
        crate::codex::description()
    }
    fn model(&self) -> ModelName {
        ModelName::named("gpt-6-luna")
    }

    async fn start(
        &self,
        bin: PathBuf,
        spec: AgentSpec,
        output: mpsc::Sender<AgentOutput>,
        tap: Option<Tap>,
    ) -> Result<Arc<dyn AgentProcess>, AgentError> {
        match tap {
            Some(tap) => crate::codex::spawn_tapped(bin, spec, output, AgentOptions::default(), tap).await,
            None => crate::codex::spawn_at(bin, spec, output, AgentOptions::default()).await,
        }
    }

    async fn scripted_model(&self, scenario: &'static Scenario) -> std::io::Result<Server> {
        super::responses::start(scenario).await
    }

    fn prepare(
        &self,
        room: &Path,
        options: &Options,
        model: Option<&Server>,
    ) -> std::io::Result<(PathBuf, Vec<(String, String)>)> {
        let Some(model) = model else { return Ok((options.cli.clone(), Vec::new())) };
        sandboxed(room, options, model)
    }

    fn named(&self, room: &Path) -> Vec<(String, String)> {
        let mut named = vec![(room.display().to_string(), "/scratch".into())];
        if let Ok(resolved) = room.canonicalize() {
            named.push((resolved.display().to_string(), "/scratch".into()));
        }
        named
    }

    /// Only the conversation this trial created, whose ID its initialization reported. No user history is enumerated.
    async fn forget(&self, options: &Options, work: &Path, id: &AgentSessionId) -> Result<(), Failed> {
        let (output, mut receiver) = mpsc::channel(64);
        let reader = tokio::spawn(async move { while receiver.recv().await.is_some() {} });
        let process = super::super::running::transport(
            &options.cli,
            work,
            &[],
            crate::registry::on(AgentKind::Codex, super::super::one_off::ONE_OFF_MODEL)
                .map_err(|e| Failed(e.to_string()))?,
            output,
            &AgentOptions::default(),
            None,
        )
        .await
        .map_err(|e| Failed(e.to_string()))?;
        let result = process.shared.rpc.ask("thread/delete", json!({"threadId":id})).await;
        process.stop().await;
        drop(process);
        let _ = reader.await;
        result.map(drop).map_err(|e| Failed(format!("removing the trial's native conversation: {e}")))
    }
}

/// The CLI a scripted trial in `room` launches, which runs Codex in macOS's sandbox, able to reach only `model`, and
/// not this machine's home, on the scripted model's provider, and the home it's launched with.
fn sandboxed(room: &Path, options: &Options, model: &Server) -> std::io::Result<(PathBuf, Vec<(String, String)>)> {
    if !cfg!(target_os = "macos") {
        return Err(std::io::Error::other("scripted Codex trials need macOS's sandbox-exec"));
    }
    let home = room.join("home");
    std::fs::create_dir_all(home.join(".codex"))?;
    let settings = json!({"model_provider":"scripted","features.code_mode_host":true,"features.shell_snapshot":false,
        "features.apps":false,"analytics.enabled":false,"model_providers.scripted.name":"Scripted",
        "model_providers.scripted.base_url":format!("{}/v1",model.url()),"model_providers.scripted.wire_api":"responses",
        "model_providers.scripted.requires_openai_auth":false,"model_providers.scripted.supports_websockets":false,
        "model_providers.scripted.request_max_retries":0,"model_providers.scripted.stream_max_retries":0});
    let mut args = String::new();
    for (key, value) in settings.as_object().into_iter().flatten() {
        let _ = write!(args, " -c {}", quoted(Path::new(&format!("{key}={value}"))));
    }
    let mut sandbox = format!(
        "(version 1)\n(allow default)\n(deny network-outbound)\n(allow network-outbound (remote ip \"localhost:{}\"))\n",
        model.port()
    );
    let cli = options.cli.canonicalize()?;
    if let Some(home) = &options.home {
        let install = cli
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| std::io::Error::other("Codex has no installation directory"))?;
        let _ = write!(
            sandbox,
            "(deny file-read* file-write* (subpath {:?}))\n(allow file-read* (subpath {:?}))\n",
            home.display().to_string(),
            install.display().to_string()
        );
    }
    let profile = room.join("sandbox.sb");
    std::fs::write(&profile, sandbox)?;
    let body = format!("#!/bin/sh\nexec /usr/bin/sandbox-exec -f {} {} \"$@\"{args}\n", quoted(&profile), quoted(&cli));
    let bin = executable(room, "codex", &body)?;
    Ok((
        bin,
        vec![
            ("HOME".into(), home.display().to_string()),
            ("CODEX_HOME".into(), home.join(".codex").display().to_string()),
            ("HTTPS_PROXY".into(), "http://127.0.0.1:9".into()),
            ("HTTP_PROXY".into(), "http://127.0.0.1:9".into()),
            ("NO_PROXY".into(), "127.0.0.1,localhost".into()),
        ],
    ))
}
