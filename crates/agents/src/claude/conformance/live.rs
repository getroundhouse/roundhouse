//! Playing a scenario on the real CLI, through the adapter, capturing what crosses its stdio.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use protocol::model::{AgentLogin, AgentSessionId, LoginToken, RunId};
use serde_json::Value;
use tokio::sync::mpsc;

use super::messages::ScriptedModel;
use super::{MODEL, fixture, project_folder, scripts, spec};
use crate::conformance::capture::{Entry, Tap};
use crate::conformance::driver::{Driver, Launcher};
use crate::conformance::scenario::{Against, Failed, Scenario};
use crate::conformance::trial::{Played, Report, Trial};
use crate::conformance::{executable, quoted};
use crate::{AgentOptions, AgentOutput, AgentProcess, BinaryLookup};

/// What a scripted trial's environment can hold of this machine's: only what the CLI and its tools need to run, so
/// nothing of the machine's login, settings or providers reaches the CLI, and what macOS gives every process, even
/// one started with nothing. A program that plays trials on the scripted model starts the process they're played in
/// with only these.
pub const KEPT_ENV: &[&str] =
    &["PATH", "TMPDIR", "LANG", "TERM", "USER", "LOGNAME", "SHELL", "__CF_USER_TEXT_ENCODING"];

/// How long a step waits for the adapter on the scripted model, which answers at once, so it's only the CLI's own
/// work, such as starting.
const SCRIPTED_PATIENCE: Duration = Duration::from_secs(30);
/// How long a step waits on the real model, which can take minutes, as it plans.
const REAL_PATIENCE: Duration = Duration::from_secs(180);
/// How often a step looks for a file the agent is to make.
const LOOK_AGAIN: Duration = Duration::from_millis(20);

/// The CLI the hub would find with `lookup`.
pub fn find_cli(lookup: &BinaryLookup) -> Option<PathBuf> {
    crate::claude::process::resolve_binary(lookup)
}

/// What a live trial uses.
#[derive(Clone, Debug)]
pub struct Options {
    pub cli: PathBuf,
    /// This machine's home, which a scripted trial's CLI can't read, and where a trial on the real model without a
    /// token leaves the files it's cleared of. `None` leaves both alone.
    pub home: Option<PathBuf>,
    /// A token `claude setup-token` made, for trials on the real model to sign in with and keep to a config of their
    /// own. Without one, they run under this machine's login and config.
    pub token: Option<LoginToken>,
}

/// How a trial on the real CLI went.
pub struct Live {
    pub report: Report,
    /// The fixture its capture makes, on the scripted model. One on the real model says what the account is and has
    /// used, so it's never kept.
    pub fixture: Option<String>,
    /// What the scripted model was asked, in order.
    pub requests: Vec<Value>,
}

/// Plays `scenario` against `against` on the real CLI, through the adapter. A trial on the scripted model has to be
/// played in a process whose environment holds only [`KEPT_ENV`].
pub async fn live(scenario: &'static Scenario, against: Against, options: &Options) -> Live {
    let started = Instant::now();
    let played = Played::Live(against);
    let (scratch, model, bin) = match prepare(scenario, against, options).await {
        Ok(prepared) => prepared,
        Err(why) => {
            let report = Report::of(scenario, played, &Trial::default(), Err(Failed(why)), Vec::new(), started);
            return Live { report, fixture: None, requests: Vec::new() };
        }
    };
    let token = options.token.clone().filter(|_| against == Against::RealModel);
    let (tap, mut entries) = Tap::new();
    let session = uuid::Uuid::new_v4().to_string();
    let real_cli = RealCli {
        scenario,
        bin,
        env: env(&scratch, model.as_ref(), token.is_some()),
        work: scratch.work(),
        token: token.clone(),
        session: AgentSessionId::from(session),
        tap,
        against,
    };
    let mut driver = Driver::new(Box::new(real_cli));
    let steps = (scenario.steps)(&mut driver).await;
    let (trial, stopped) = driver.finish().await;
    let mut captured = Vec::new();
    while let Ok(entry) = entries.try_recv() {
        captured.push(entry);
    }
    let models = trial.models();
    let only_the_chats = if !models.is_empty() && models.iter().all(|model| **model == MODEL) {
        Ok(())
    } else {
        Err(format!("they came from {models:?}, and the chat runs {MODEL:?}"))
    };
    let also = vec![("each reply came from the model the chat runs", only_the_chats)];
    let report = Report::of(scenario, played, &trial, steps.and(stopped), also, started);
    let fixture =
        (against == Against::ScriptedModel).then(|| fixture::fixture(&captured, scratch.named(), &scratch.plans()));
    if let Some(home) = options.home.as_deref().filter(|_| against == Against::RealModel && token.is_none()) {
        forget(home, &scratch.work(), &captured);
    }
    Live { report, fixture, requests: model.map_or_default(|model| model.requests()) }
}

/// What a live trial needs before its first step: the scratch project, the scripted model when it's against that, and
/// the CLI to launch, sandboxed there.
async fn prepare(
    scenario: &'static Scenario,
    against: Against,
    options: &Options,
) -> Result<(Scratch, Option<ScriptedModel>, PathBuf), String> {
    let scratch = Scratch::make().map_err(|e| format!("making the scratch project: {e}"))?;
    match against {
        Against::ScriptedModel => {
            let script = scripts::script(scenario).ok_or("the scenario has no script for Claude's scripted model")?;
            let model = ScriptedModel::start(script).await.map_err(|e| format!("starting the scripted model: {e}"))?;
            let bin = sandboxed(&scratch, &options.cli, model.port(), options.home.as_deref())
                .map_err(|e| format!("sandboxing the CLI: {e}"))?;
            Ok((scratch, Some(model), bin))
        }
        Against::RealModel => Ok((scratch, None, options.cli.clone())),
    }
}

/// What a fixture calls a trial's scratch directory.
const SCRATCH: &str = "/scratch";

/// Where a trial's project and home are, outside every checkout, so the CLI finds no repository's instructions or
/// settings but its own. Removed once the trial has finished.
struct Scratch {
    dir: tempfile::TempDir,
}

impl Scratch {
    fn make() -> std::io::Result<Scratch> {
        let dir = tempfile::Builder::new().prefix("claude-conformance-").tempdir()?;
        let scratch = Scratch { dir };
        std::fs::create_dir_all(scratch.config())?;
        std::fs::create_dir(scratch.work())?;
        std::fs::write(scratch.work().join("README.md"), "A scratch project.\n")?;
        let git = Command::new("git").args(["init", "-q"]).arg(scratch.work()).env("HOME", scratch.home()).status()?;
        if !git.success() {
            return Err(std::io::Error::other("git init failed"));
        }
        Ok(scratch)
    }

    fn work(&self) -> PathBuf {
        self.dir.path().join("work")
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    /// Its directory, as given and as the OS resolves it, such as macOS's `/private/var` for `/var`, and as the name
    /// Claude gives a project's folder, each with the name a fixture gives it, since it differs every trial.
    fn named(&self) -> Vec<(String, String)> {
        Self::as_given_and_resolved(self.dir.path())
            .iter()
            .flat_map(|path| {
                let path = path.to_string_lossy().into_owned();
                [(project_folder(&path), project_folder(SCRATCH)), (path, SCRATCH.to_string())]
            })
            .collect()
    }

    /// The CLI's config directory, which is its own.
    fn config(&self) -> PathBuf {
        self.home().join(".claude")
    }

    /// Where the CLI keeps its plans, as given and as the OS resolves it.
    fn plans(&self) -> Vec<PathBuf> {
        Self::as_given_and_resolved(&self.config()).iter().map(|config| config.join("plans")).collect()
    }

    fn as_given_and_resolved(path: &Path) -> [PathBuf; 2] {
        let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        [resolved, path.to_path_buf()]
    }
}

/// What a scripted trial's CLI can reach, read and write: the scripted model on its `port`, and nothing else on the
/// network, not even a Unix socket, such as another Claude session's, and nothing of `home`, not even the directory
/// itself, but the CLI, which it can only read.
fn profile(port: &str, home: Option<&Path>, cli: &Path) -> String {
    let mut profile = format!(
        "(version 1)\n(allow default)\n(deny network-outbound)\n(allow network-outbound (remote ip \"localhost:{port}\"))\n(deny network-bind (local unix-socket))\n"
    );
    if let Some(home) = home {
        let resolved = cli.canonicalize().unwrap_or_else(|_| cli.to_path_buf());
        let literal = |path: &Path| format!("(literal {:?})", path.display().to_string());
        let home = home.display().to_string();
        let (cli, resolved) = (literal(cli), literal(&resolved));
        profile += &[
            format!("(deny file-read* file-write* (subpath {home:?}))"),
            format!("(allow file-read* {cli} {resolved})"),
        ]
        .join("\n");
        profile.push('\n');
    }
    profile
}

/// The CLI, run under `sandbox-exec` with [`profile`], by a script the adapter runs in its place, which execs it, so
/// the adapter's process is the CLI's.
fn sandboxed(scratch: &Scratch, cli: &Path, port: &str, home: Option<&Path>) -> std::io::Result<PathBuf> {
    if !cfg!(target_os = "macos") {
        return Err(std::io::Error::other(
            "a scripted trial needs macOS's sandbox-exec, which keeps it off the network",
        ));
    }
    let profile_file = scratch.dir.path().join("sandbox.sb");
    std::fs::write(&profile_file, profile(port, home, cli))?;
    let body = format!("#!/bin/sh\nexec /usr/bin/sandbox-exec -f {} {} \"$@\"\n", quoted(&profile_file), quoted(cli));
    executable(scratch.dir.path(), "claude", &body)
}

/// What the CLI is given beyond what it inherits: on the scripted model, a home of its own and the model, with a key
/// that's no one's, and on the real one, a home of its own only when it signs in with a token. Each turns off what
/// the CLI would otherwise fetch or report as it starts.
fn env(scratch: &Scratch, model: Option<&ScriptedModel>, own_home: bool) -> Vec<(String, String)> {
    let path = |path: PathBuf| path.to_string_lossy().into_owned();
    let mut env = vec![
        ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1".to_string()),
        ("DISABLE_AUTOUPDATER", "1".to_string()),
        ("TZ", "UTC".to_string()),
    ];
    if model.is_some() || own_home {
        env.extend([("HOME", path(scratch.home())), ("CLAUDE_CONFIG_DIR", path(scratch.config()))]);
    }
    if let Some(model) = model {
        // Any proxy is one that refuses, so nothing reaches past the scripted model even without the sandbox.
        let refused = "http://127.0.0.1:9".to_string();
        env.extend([
            ("ANTHROPIC_BASE_URL", model.url().to_string()),
            ("ANTHROPIC_API_KEY", "sk-ant-scripted-not-a-real-key".to_string()),
            ("HTTPS_PROXY", refused.clone()),
            ("HTTP_PROXY", refused),
            ("NO_PROXY", "127.0.0.1,localhost".to_string()),
        ]);
    }
    env.into_iter().map(|(name, value)| (name.to_string(), value)).collect()
}

/// Removes what a trial on the real model left in `home`'s `.claude`: its conversation's folder, which the CLI names
/// by the project's path and shortens when it's long, and each plan it wrote, which the capture names.
fn forget(home: &Path, work: &Path, captured: &[Entry]) {
    let claude = home.join(".claude");
    let resolved = work.canonicalize().unwrap_or_else(|_| work.to_path_buf());
    let named = project_folder(&resolved.to_string_lossy());
    let tail = &named[named.len().saturating_sub(40)..];
    // Best effort: what's left is only files the person can delete themselves.
    for entry in std::fs::read_dir(claude.join("projects")).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().contains(tail) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    let plans = claude.join("plans");
    let text = serde_json::to_string(captured).unwrap_or_default();
    for name in fixture::plan_files(&text, &format!("{}/", plans.display())) {
        let _ = std::fs::remove_file(plans.join(name));
    }
}

/// Launches the real CLI, capturing what crosses its stdio.
struct RealCli {
    scenario: &'static Scenario,
    bin: PathBuf,
    env: Vec<(String, String)>,
    work: PathBuf,
    token: Option<LoginToken>,
    session: AgentSessionId,
    tap: Tap,
    against: Against,
}

#[async_trait]
impl Launcher for RealCli {
    async fn launch(
        &mut self,
        run: RunId,
        resume: bool,
        output: mpsc::Sender<AgentOutput>,
    ) -> Result<Arc<dyn AgentProcess>, Failed> {
        let spec = spec(self.scenario, run, &self.work, &self.session, resume, self.env.clone())?;
        let options =
            AgentOptions { login: self.token.clone().map(|token| AgentLogin { token }), ..AgentOptions::default() };
        let spawned = crate::claude::spawn_tapped(self.bin.clone(), spec, output, options, self.tap.clone()).await;
        spawned.map_err(|e| Failed(format!("launching the CLI: {e}")))
    }

    async fn made(&mut self, name: &str) -> Result<(), Failed> {
        let path = self.work.join(name);
        let deadline = Instant::now() + self.patience();
        while !path.exists() {
            if Instant::now() > deadline {
                return Err(Failed(format!("timed out waiting for the agent to make {name}")));
            }
            tokio::time::sleep(LOOK_AGAIN).await;
        }
        self.tap.note(Entry::Saw(name.to_string()));
        Ok(())
    }

    fn patience(&self) -> Duration {
        match self.against {
            Against::ScriptedModel => SCRIPTED_PATIENCE,
            Against::RealModel => REAL_PATIENCE,
        }
    }
}
