use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::worker::AgentSpec;
use tokio::sync::{mpsc, watch};

use super::{launch, rpc::Rpc, state::Conversation, supervise};
use crate::run::Run;
use crate::{AgentError, AgentOptions, AgentOutput, AgentProcess};

pub(super) struct Shared {
    pub(super) rpc: Rpc,
    pub(super) state: Mutex<Conversation>,
    pub(super) controls: tokio::sync::Mutex<()>,
    pub(super) run: Run,
    pub(super) turn_done: watch::Sender<Option<super::state::TurnId>>,
    pub(super) tap: Option<crate::tap::Tap>,
}

pub(super) struct CodexProcess {
    pub(super) shared: Arc<Shared>,
}

impl Drop for CodexProcess {
    fn drop(&mut self) {
        self.shared.run.kill_unless_gone();
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
    Ok(start_conversation(&bin, &spec, output, &options, launch::Purpose::Agent, None).await?)
}

#[cfg(any(test, feature = "testing"))]
pub async fn spawn_tapped(
    bin: PathBuf,
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
    tap: crate::conformance::capture::Tap,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    Ok(start_conversation(&bin, &spec, output, &options, launch::Purpose::Agent, Some(tap)).await?)
}

pub(super) async fn start_conversation(
    bin: &Path,
    spec: &AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: &AgentOptions,
    purpose: launch::Purpose,
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
    let settings = crate::registry::launch(spec)?;
    let process = transport(bin, Path::new(&spec.cwd), &spec.env, settings, output, options, tap).await?;
    if let Err(error) = launch::conversation(&process.shared, spec, options, purpose).await {
        process.shared.run.stop().await;
        return Err(error);
    }
    Ok(process)
}

pub(super) async fn transport(
    bin: &Path,
    cwd: &Path,
    env: &[(String, String)],
    settings: crate::registry::Launch,
    output: mpsc::Sender<AgentOutput>,
    options: &AgentOptions,
    tap: Option<crate::tap::Tap>,
) -> Result<Arc<CodexProcess>, AgentError> {
    let launched = crate::starting::start_piped("Codex", bin, &launch::ARGS, cwd, env, options)?;
    let shared = Arc::new(Shared {
        rpc: Rpc::new(launched.stdin, super::rpc::Dialect::Bare, "Codex", tap.clone()),
        state: Mutex::new(Conversation::new(settings)),
        controls: tokio::sync::Mutex::new(()),
        run: Run::new(launched.pid, options.pid_file.clone(), output),
        turn_done: watch::channel(None).0,
        tap,
    });
    tokio::spawn(supervise::supervise(launched.child, launched.stdout, launched.stderr, shared.clone()));
    let process = Arc::new(CodexProcess { shared });
    if let Err(error) = launch::initialize(&process.shared).await {
        process.shared.run.stop().await;
        return Err(error);
    }
    Ok(process)
}
