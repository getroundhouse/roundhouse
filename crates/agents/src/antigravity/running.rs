use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::AtomicBool};

use parking_lot::Mutex;
use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings;
use protocol::model::ModelName;
use protocol::paths::ENV_ANTIGRAVITY_BIN;
use protocol::worker::AgentSpec;
use tokio::sync::{mpsc, watch};

use super::state::{Conversation, Ending};
use super::{launch, supervise};
use crate::jsonrpc::{Dialect, Rpc};
use crate::run::Run;
use crate::{AgentError, AgentOptions, AgentOutput, AgentProcess, acp};
use tracing::Instrument as _;

pub(super) struct LaunchContext {
    pub(super) bin: PathBuf,
    pub(super) cwd: PathBuf,
    pub(super) env: Vec<(String, String)>,
    pub(super) options: AgentOptions,
    pub(super) purpose: launch::Purpose,
}

/// The CLI the run is in now: whether it's being replaced, and whether it has exited.
pub(super) struct ChildSlot {
    pub(super) replacing: Arc<AtomicBool>,
    pub(super) exited: watch::Receiver<bool>,
}

pub(super) struct Shared {
    pub(super) rpc_slot: Mutex<Arc<Rpc>>,
    pub(super) child: Mutex<ChildSlot>,
    pub(super) context: LaunchContext,
    pub(super) state: Mutex<Conversation>,
    pub(super) controls: tokio::sync::Mutex<()>,
    pub(super) run: Run,
    /// How many times a turn's end, or its plan, has been recorded, for whatever waits on one. It changes only under
    /// the lock on `state`, with `Conversation.ending`, by `count_end`.
    pub(super) turn_done: watch::Sender<u64>,
    pub(super) tap: Option<crate::tap::Tap>,
}

impl Shared {
    pub(super) fn rpc(&self) -> Arc<Rpc> {
        self.rpc_slot.lock().clone()
    }

    /// Sets how far the turn has got to ending, once its end or its plan is recorded, and counts that for whatever
    /// waits on it, under the lock `state` is held by, so whoever reads both under it finds them agree.
    pub(super) fn count_end(&self, state: &mut Conversation, ending: Ending) {
        state.ending = ending;
        self.turn_done.send_modify(|n| *n = n.saturating_add(1));
    }
}

pub(super) struct AntigravityProcess {
    pub(super) shared: Arc<Shared>,
}

impl Drop for AntigravityProcess {
    fn drop(&mut self) {
        self.shared.run.kill_unless_gone();
    }
}

pub(super) fn binary(options: &AgentOptions) -> Result<PathBuf, AgentError> {
    crate::binary::resolve_binary("agy", &[], &options.binary).ok_or_else(|| {
        AgentError::Refused(format!(
            "Could not find the Antigravity CLI. Install agy, or set {ENV_ANTIGRAVITY_BIN} to its path"
        ))
    })
}

pub(super) fn mode_id(mode: ApprovalMode) -> &'static str {
    if mode.plan { "plan" } else { "accept-edits" }
}

pub async fn spawn(
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    spawn_at(binary(&options)?, spec, output, options).await
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
) -> Result<Arc<AntigravityProcess>, AgentError> {
    let claims = super::claims::claims();
    if !spec.start.allowed(&claims) || !spec.settings.allowed(&claims) {
        return Err(AgentError::Refused("Antigravity does not offer those launch settings".into()));
    }
    let crate::registry::Launch { model, mode, effort } = crate::registry::launch(spec)?;
    effort_str(&effort)?;
    ensure_model_offers_effort(&model, &effort)?;
    let args = purpose.args(mode);
    if let Some(tap) = &tap {
        let conversation = match &spec.start {
            protocol::capability::conversation::Start::Resume { conversation } => Some(conversation),
            protocol::capability::conversation::Start::New { .. }
            | protocol::capability::conversation::Start::Fork { .. }
            | protocol::capability::conversation::Start::Import { .. } => None,
        };
        let owned: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        tap.launched(conversation.is_some(), conversation, &owned);
    }
    let context = LaunchContext {
        bin: bin.to_path_buf(),
        cwd: PathBuf::from(&spec.cwd),
        env: spec.env.clone(),
        options: options.clone(),
        purpose,
    };
    let hold_plans = matches!(purpose, launch::Purpose::Agent);
    let skip_permissions = purpose.skips_permissions(mode);
    let (process, agent_version) =
        transport(context, args, (model, mode, effort, hold_plans, skip_permissions), output, tap).await?;
    if let Err(error) = launch::conversation(&process.shared, spec, options, purpose, agent_version).await {
        process.shared.run.stop().await;
        return Err(error);
    }
    Ok(process)
}

async fn transport(
    context: LaunchContext,
    args: &[&str],
    initial: (ModelName, ApprovalMode, settings::Effort, bool, bool),
    output: mpsc::Sender<AgentOutput>,
    tap: Option<crate::tap::Tap>,
) -> Result<(Arc<AntigravityProcess>, Option<String>), AgentError> {
    let launched =
        crate::starting::start_piped("Antigravity", &context.bin, args, &context.cwd, &context.env, &context.options)?;
    let run = Run::new(launched.pid, context.options.pid_file.clone(), output);
    let rpc = Arc::new(Rpc::new(launched.stdin, Dialect::JsonRpc2, "Antigravity", tap.clone()));
    let replacing = Arc::new(AtomicBool::new(false));
    let child_gone = Arc::new(AtomicBool::new(false));
    let (exited_tx, exited_rx) = watch::channel(false);
    let shared = Arc::new(Shared {
        rpc_slot: Mutex::new(rpc.clone()),
        child: Mutex::new(ChildSlot { replacing: replacing.clone(), exited: exited_rx }),
        context,
        state: Mutex::new(Conversation::new(initial.0, initial.1, initial.2, initial.3, initial.4)),
        controls: tokio::sync::Mutex::new(()),
        run,
        turn_done: watch::channel(0).0,
        tap,
    });
    tokio::spawn(
        supervise::supervise(
            supervise::ChildPipes {
                child: launched.child,
                stdout: launched.stdout,
                stderr: launched.stderr,
                rpc,
                replacing,
                child_gone,
                exited_tx,
            },
            shared.clone(),
        )
        .in_current_span(),
    );
    let process = Arc::new(AntigravityProcess { shared });
    let agent_version = match launch::initialize(&process.shared).await {
        Ok(version) => version,
        Err(error) => {
            process.shared.run.stop().await;
            return Err(error);
        }
    };
    Ok((process, agent_version))
}

pub(super) fn ensure_model_offers_effort(model: &ModelName, effort: &settings::Effort) -> Result<(), AgentError> {
    if super::description::models()
        .offered()
        .iter()
        .find(|offer| offer.model == *model)
        .is_some_and(|offer| !offer.efforts.iter().any(|e| e.id() == effort))
    {
        return Err(AgentError::Refused("This Antigravity model does not offer that effort".into()));
    }
    Ok(())
}

/// Refuses a model the conversation's `model` option doesn't list, which is what this machine's sign-in offers, rather
/// than leave `agy` to answer `Unknown model ID`. The catalog's Claude models come only with a Google account's
/// sign-in, so a Gemini API key's conversation doesn't list them. A conversation that lists no models is left to `agy`.
pub(super) fn ensure_sign_in_offers_model(options: &[acp::ConfigOption], model: &ModelName) -> Result<(), AgentError> {
    let listed = options.iter().any(|option| option.id == "model");
    if !listed || acp::config_offers_value(options, "model", model.as_str()) {
        return Ok(());
    }
    Err(AgentError::Refused(if model.as_str().starts_with("claude-") {
        "Antigravity offers Claude models only when it's signed in with a Google account, not a Gemini API key".into()
    } else {
        "Antigravity's sign-in on this machine doesn't offer this model".into()
    }))
}

pub(super) fn effort_str(effort: &settings::Effort) -> Result<&'static str, AgentError> {
    match effort {
        settings::Effort::Low => Ok("low"),
        settings::Effort::Medium => Ok("medium"),
        settings::Effort::High => Ok("high"),
        settings::Effort::Xhigh => Ok("xhigh"),
        settings::Effort::None
        | settings::Effort::Minimal
        | settings::Effort::Max
        | settings::Effort::Ultra
        | settings::Effort::Persistent
        | settings::Effort::Unset
        | settings::Effort::Named(_)
        | settings::Effort::Unknown => Err(AgentError::UnknownEffort),
    }
}
