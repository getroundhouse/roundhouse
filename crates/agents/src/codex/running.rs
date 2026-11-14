use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use parking_lot::Mutex;
use protocol::event::EventBody;
use protocol::model::SessionSettings;
use protocol::worker::AgentSpec;
use tokio::sync::{mpsc, watch};

use super::{launch, rpc::Rpc, state::Conversation, supervise};
use crate::{AgentError, AgentOptions, AgentOutput, AgentProcess};

const STOP_GRACE: Duration = Duration::from_secs(3);

pub(super) struct Shared {
    pub(super) rpc: Rpc,
    pub(super) state: Mutex<Conversation>,
    pub(super) controls: tokio::sync::Mutex<()>,
    pub(super) pid: u32,
    pub(super) gone: AtomicBool,
    pub(super) requested: AtomicBool,
    pub(super) exited: watch::Sender<bool>,
    pub(super) turn_done: watch::Sender<Option<super::state::TurnId>>,
    pub(super) tap: Option<crate::tap::Tap>,
    emission: tokio::sync::Mutex<Option<mpsc::Sender<AgentOutput>>>,
}

impl Shared {
    pub(super) async fn emit(&self, output: AgentOutput) {
        let mut sender = self.emission.lock().await;
        let closes = matches!(output, AgentOutput::Exited { .. });
        if let Some(sender) = sender.as_ref() {
            let _ = sender.send(output).await;
        }
        if closes {
            *sender = None;
        }
    }

    pub(super) async fn event(&self, body: EventBody, raw: Option<String>) {
        self.emit(AgentOutput::event(body, raw)).await;
    }

    pub(super) async fn stop(&self) {
        self.requested.store(true, Ordering::SeqCst);
        let mut exited = self.exited.subscribe();
        if *exited.borrow() {
            return;
        }
        process::signal_group(self.pid, process::Signal::Terminate);
        if tokio::time::timeout(STOP_GRACE, exited.wait_for(|done| *done)).await.is_err() {
            process::signal_group(self.pid, process::Signal::Kill);
            let _ = exited.wait_for(|done| *done).await;
        }
    }
}

pub(super) struct CodexProcess {
    pub(super) shared: Arc<Shared>,
}

impl Drop for CodexProcess {
    fn drop(&mut self) {
        // The supervisor owns the child, but only this handle owns its lifetime. Cancellation of a startup or
        // one-off future must kill the group even before initialization has answered.
        if !self.shared.gone.load(Ordering::SeqCst) {
            self.shared.requested.store(true, Ordering::SeqCst);
            process::signal_group(self.shared.pid, process::Signal::Kill);
        }
    }
}

pub async fn spawn(
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    let bin = crate::binary::resolve_binary("codex", &[], &options.binary).ok_or_else(|| {
        AgentError::Refused(
            "Could not find the Codex CLI. Install codex, or set ROUNDHOUSE_CODEX_BIN to its path".into(),
        )
    })?;
    spawn_at(bin, spec, output, options).await
}

pub async fn spawn_at(
    bin: PathBuf,
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    Ok(start_conversation(&bin, &spec, output, &options, false, None).await?)
}

#[cfg(feature = "testing")]
pub async fn spawn_tapped(
    bin: PathBuf,
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
    tap: crate::conformance::capture::Tap,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    Ok(start_conversation(&bin, &spec, output, &options, false, Some(tap)).await?)
}

pub(super) async fn start_conversation(
    bin: &Path,
    spec: &AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: &AgentOptions,
    ephemeral: bool,
    tap: Option<crate::tap::Tap>,
) -> Result<Arc<CodexProcess>, AgentError> {
    if let Some(tap) = &tap {
        let conversation = match &spec.start {
            protocol::capability::conversation::Start::Resume { conversation } => Some(conversation),
            protocol::capability::conversation::Start::New { .. }
            | protocol::capability::conversation::Start::Fork { .. }
            | protocol::capability::conversation::Start::Import { .. } => None,
        };
        tap.launched(conversation.is_some(), conversation, &launch::ARGS.map(str::to_owned));
    }
    let process = transport(bin, Path::new(&spec.cwd), &spec.env, spec.settings.clone(), output, options, tap).await?;
    if let Err(error) = launch::conversation(&process.shared, spec, options, ephemeral).await {
        process.shared.stop().await;
        return Err(error);
    }
    Ok(process)
}

pub(super) async fn transport(
    bin: &Path,
    cwd: &Path,
    env: &[(String, String)],
    settings: SessionSettings,
    output: mpsc::Sender<AgentOutput>,
    options: &AgentOptions,
    tap: Option<crate::tap::Tap>,
) -> Result<Arc<CodexProcess>, AgentError> {
    let launched = launch::launch(bin, cwd, env, options)?;
    let shared = Arc::new(Shared {
        rpc: Rpc::new(launched.stdin, tap.clone()),
        state: Mutex::new(Conversation::new(settings)),
        controls: tokio::sync::Mutex::new(()),
        pid: launched.pid,
        gone: AtomicBool::new(false),
        requested: AtomicBool::new(false),
        exited: watch::channel(false).0,
        turn_done: watch::channel(None).0,
        tap,
        emission: tokio::sync::Mutex::new(Some(output)),
    });
    tokio::spawn(supervise::supervise(
        launched.child,
        launched.stdout,
        launched.stderr,
        shared.clone(),
        options.pid_file.clone(),
    ));
    let process = Arc::new(CodexProcess { shared });
    if let Err(error) = launch::initialize(&process.shared).await {
        process.shared.stop().await;
        return Err(error);
    }
    Ok(process)
}
