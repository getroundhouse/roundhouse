use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::Mutex;
use protocol::{
    capability::conversation::Start,
    capability::settings::Effort,
    event::EventBody,
    model::{AgentKind, AgentSessionId, ModelName, RunId},
    worker::AgentSpec,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::conformance::{
    capture::{self, Entry, Tap},
    driver::{Driver, Launcher},
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
    crate::binary::resolve_binary("agy", &[], lookup)
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
    let room = tempfile::Builder::new().prefix("antigravity-conformance-").tempdir().map_err(|e| e.to_string())?;
    let work = room.path().join("work");
    std::fs::create_dir(&work).map_err(|e| e.to_string())?;
    std::fs::write(work.join("README.md"), "A scratch project.\n").map_err(|e| e.to_string())?;
    let model = if against == Against::ScriptedModel {
        Some(super::gemini::start(scenario).await.map_err(|e| e.to_string())?)
    } else {
        None
    };
    let env = prepare(room.path(), model.as_ref()).map_err(|e| e.to_string())?;
    let (tap, mut captured) = Tap::new();
    let conversation = Arc::new(Mutex::new(None));
    let launcher = RealCli {
        bin: options.cli.clone(),
        work: work.clone(),
        env,
        scenario,
        against,
        conversation: conversation.clone(),
        tap,
    };
    let mut driver = Driver::new(Box::new(launcher));
    let steps = (scenario.steps)(&mut driver).await;
    let (trial, stopped) = driver.finish().await;
    let cleanup_id = (against == Against::RealModel).then(|| conversation.lock().clone()).flatten();
    if let Some(files) = cleanup_id
        .zip(options.home.as_deref())
        .and_then(|(id, home)| super::super::files::ConversationFiles::of(home, &id))
    {
        files.remove().await;
    }
    let mut entries = Vec::new();
    while let Ok(mut entry) = captured.try_recv() {
        scrub_timestamps(&mut entry);
        entries.push(entry);
    }
    let report = Report::of(scenario, Played::Live(against), &trial, steps.and(stopped), Vec::new(), started);
    let fixture = (against == Against::ScriptedModel).then(|| {
        let mut named = Vec::new();
        if let Ok(resolved) = room.path().canonicalize() {
            named.push((resolved.display().to_string(), "/scratch".into()));
        }
        named.push((room.path().display().to_string(), "/scratch".into()));
        capture::fixture(&entries, &named, &super::wire::JsonRpc)
    });
    Ok(Live { report, fixture, requests: model.map_or_default(|model| model.requests()) })
}

fn prepare(room: &Path, model: Option<&crate::conformance::http::Server>) -> std::io::Result<Vec<(String, String)>> {
    let Some(model) = model else { return Ok(Vec::new()) };
    let home = room.join("home");
    let config_dir = home.join(".gemini").join("antigravity-cli");
    std::fs::create_dir_all(&config_dir)?;
    std::fs::write(config_dir.join("settings.json"), json!({ "modelProvider": "gemini" }).to_string())?;
    Ok(vec![
        ("HOME".into(), home.display().to_string()),
        ("GEMINI_API_KEY".into(), "scripted".into()),
        ("GOOGLE_GEMINI_BASE_URL".into(), model.url().into()),
        ("HTTPS_PROXY".into(), "http://127.0.0.1:9".into()),
        ("HTTP_PROXY".into(), "http://127.0.0.1:9".into()),
        ("NO_PROXY".into(), "127.0.0.1,localhost".into()),
    ])
}

fn scrub_timestamps(entry: &mut Entry) {
    if let Entry::FromCli(value) = entry {
        scrub_value(value);
    }
}

fn scrub_value(value: &mut Value) {
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
        Value::Array(items) => items.iter_mut().for_each(scrub_value),
        Value::Object(map) => map.values_mut().for_each(scrub_value),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
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
                    .ok_or_else(|| Failed("Antigravity reported no conversation".into()))?,
            }
        } else {
            Start::New { conversation: None }
        };
        let mut settings = crate::antigravity::description().defaults;
        settings.model = ModelName::named(super::super::description::TRIAL_MODEL);
        settings.effort = Effort::Low;
        settings.permission_mode = self.scenario.mode;
        let claims = crate::antigravity::description().capabilities.unwrap_or_default();
        let settings = settings.for_capabilities(&claims).map_err(|e| Failed(e.to_string()))?;
        let spec = AgentSpec {
            session_id: "conformance".into(),
            run,
            agent: AgentKind::Antigravity,
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
        crate::antigravity::spawn_tapped(self.bin.clone(), spec, sender, AgentOptions::default(), self.tap.clone())
            .await
            .map_err(|e| Failed(e.to_string()))
    }
    async fn made(&mut self, name: &str) -> Result<(), Failed> {
        if !waiting::task::up_to(self.patience(), || self.work.join(name).exists()).await {
            return Err(Failed(format!("Antigravity did not make {name}")));
        }
        self.tap.note(Entry::Saw(name.into()));
        Ok(())
    }
    fn patience(&self) -> Duration {
        Duration::from_secs(if self.against == Against::ScriptedModel { 30 } else { 180 })
    }
}
