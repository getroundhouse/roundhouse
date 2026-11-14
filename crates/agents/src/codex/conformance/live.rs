use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::Mutex;
use protocol::{
    capability::conversation::Start,
    event::EventBody,
    model::{AgentKind, AgentSessionId, RunId},
    worker::AgentSpec,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::conformance::{
    capture::{self, Entry, Tap},
    driver::{Driver, Launcher},
    executable, quoted,
    scenario::{Against, Failed, Scenario},
    trial::{Played, Report, Trial},
};
use crate::{AgentOptions, AgentOutput, AgentProcess, BinaryLookup};

pub struct Options {
    pub cli: PathBuf,
    pub home: Option<PathBuf>,
}

pub struct Live {
    pub report: Report,
    pub fixture: Option<String>,
    pub requests: Vec<Value>,
}

pub fn find_cli(lookup: &BinaryLookup) -> Option<PathBuf> {
    crate::binary::resolve_binary("codex", &[], lookup)
}

pub async fn live(scenario: &'static Scenario, against: Against, options: &Options) -> Live {
    let started = Instant::now();
    match play(scenario, against, options, started).await {
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
    scenario: &'static Scenario,
    against: Against,
    options: &Options,
    started: Instant,
) -> Result<Live, String> {
    let room = tempfile::Builder::new().prefix("codex-conformance-").tempdir().map_err(|e| e.to_string())?;
    let work = room.path().join("work");
    std::fs::create_dir(&work).map_err(|e| e.to_string())?;
    std::fs::write(work.join("README.md"), "A scratch project.\n").map_err(|e| e.to_string())?;
    let model = if against == Against::ScriptedModel {
        Some(super::responses::start(scenario).await.map_err(|e| e.to_string())?)
    } else {
        None
    };
    let (bin, env) = prepare(room.path(), options, model.as_ref()).map_err(|e| e.to_string())?;
    let (tap, mut captured) = Tap::new();
    let conversation = Arc::new(Mutex::new(None));
    let launcher = RealCli { bin, work: work.clone(), env, scenario, against, conversation: conversation.clone(), tap };
    let mut driver = Driver::new(Box::new(launcher));
    let steps = (scenario.steps)(&mut driver).await;
    let (trial, stopped) = driver.finish().await;
    let cleanup = if against == Against::RealModel {
        let id = conversation.lock().clone();
        match id {
            Some(id) => delete_trial(&options.cli, &work, &id).await,
            None => Ok(()),
        }
    } else {
        Ok(())
    };
    let mut entries = Vec::new();
    while let Ok(entry) = captured.try_recv() {
        entries.push(entry);
    }
    let report =
        Report::of(scenario, Played::Live(against), &trial, steps.and(stopped).and(cleanup), Vec::new(), started);
    let fixture = (against == Against::ScriptedModel).then(|| {
        let mut named = vec![(room.path().display().to_string(), "/scratch".into())];
        if let Ok(resolved) = room.path().canonicalize() {
            named.push((resolved.display().to_string(), "/scratch".into()));
        }
        capture::fixture(&entries, &named, &super::wire::JsonRpc)
    });
    Ok(Live { report, fixture, requests: model.map_or_default(|model| model.requests()) })
}

fn prepare(
    room: &Path,
    options: &Options,
    model: Option<&crate::conformance::http::Server>,
) -> std::io::Result<(PathBuf, Vec<(String, String)>)> {
    let Some(model) = model else { return Ok((options.cli.clone(), Vec::new())) };
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

struct RealCli {
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
            Start::Resume {
                conversation: self
                    .conversation
                    .lock()
                    .clone()
                    .ok_or_else(|| Failed("Codex reported no conversation".into()))?,
            }
        } else {
            Start::New { conversation: None }
        };
        let mut settings = crate::codex::description().defaults;
        settings.model = protocol::model::ModelName::named("gpt-6-luna");
        settings.effort = protocol::model::Effort::Low;
        settings.permission_mode = self.scenario.mode;
        // As the hub converts a chat's settings, against what the kind claims.
        let claims = crate::codex::description().capabilities.unwrap_or_default();
        let settings = settings.for_capabilities(&claims).map_err(|e| Failed(e.to_string()))?;
        let spec = AgentSpec {
            session_id: "conformance".into(),
            run,
            agent: AgentKind::Codex,
            cwd: self.work.display().to_string(),
            start,
            settings,
            env: self.env.clone(),
            tools_token: None,
            login: None,
        };
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
        crate::codex::spawn_tapped(self.bin.clone(), spec, sender, AgentOptions::default(), self.tap.clone())
            .await
            .map_err(|e| Failed(e.to_string()))
    }
    async fn made(&mut self, name: &str) -> Result<(), Failed> {
        let deadline = Instant::now() + self.patience();
        while !self.work.join(name).exists() {
            if Instant::now() > deadline {
                return Err(Failed(format!("Codex did not make {name}")));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        self.tap.note(Entry::Saw(name.into()));
        Ok(())
    }
    fn patience(&self) -> Duration {
        Duration::from_secs(if self.against == Against::ScriptedModel { 30 } else { 180 })
    }
}

/// Only the conversation this trial created, whose ID its initialization reported. No user history is enumerated.
async fn delete_trial(bin: &Path, cwd: &Path, id: &AgentSessionId) -> Result<(), Failed> {
    let (output, mut receiver) = mpsc::channel(64);
    let reader = tokio::spawn(async move { while receiver.recv().await.is_some() {} });
    let process = super::super::running::transport(
        bin,
        cwd,
        &[],
        crate::codex::description().defaults,
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
