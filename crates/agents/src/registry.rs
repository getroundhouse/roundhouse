use crate::{AgentError, AgentOptions, AgentOutput, AgentProcess, claude, codex};
use protocol::attachment::NewAttachment;
use protocol::model::{AgentAccount, AgentDescription, AgentKind, Title};
use protocol::prompt::Prompt;
use protocol::worker::AgentSpec;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// What each kind of agent this build runs is called and offers, as its adapter describes it, for the hub to give each
/// chat what its agent starts with and to tell clients what to offer.
pub fn descriptions() -> Vec<AgentDescription> {
    vec![claude::description()]
}

pub async fn spawn(
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    let description = descriptions().into_iter().find(|d| d.kind == spec.agent).ok_or(AgentError::UnknownKind)?;
    let claims = description.capabilities.as_ref().ok_or(AgentError::UnknownKind)?;
    let settings = spec.settings.for_capabilities(claims).map_err(|e| AgentError::Refused(e.to_string()))?;
    if !spec.start.allowed(claims) || !settings.allowed(claims) {
        return Err(AgentError::Refused("The agent does not offer these launch settings".into()));
    }
    match spec.agent {
        AgentKind::Claude => claude::spawn(spec, output, options).await,
        AgentKind::Codex => codex::spawn(spec, output, options).await,
        AgentKind::Unknown => Err(AgentError::UnknownKind),
    }
}

/// The account `kind`'s CLI runs under here, with `options.login` or with this machine's own, from a one-off call
/// that runs in `scratch_dir`.
pub async fn account(kind: AgentKind, options: &AgentOptions, scratch_dir: &Path) -> Result<AgentAccount, AgentError> {
    match kind {
        AgentKind::Claude => claude::account(options, scratch_dir).await,
        AgentKind::Codex => codex::account(options, scratch_dir).await,
        AgentKind::Unknown => Err(AgentError::UnknownKind),
    }
}

/// A short title for a prompt, with the images it carried, from a one-off call to `kind`'s CLI that runs in an empty
/// directory made inside `scratch_dir`. `options.pid_file` records it while it runs.
pub async fn title_for(
    kind: AgentKind,
    prompt: &Prompt<NewAttachment>,
    options: &AgentOptions,
    scratch_dir: &Path,
) -> Result<Title, AgentError> {
    match kind {
        AgentKind::Claude => claude::title_for(prompt, options, scratch_dir).await,
        AgentKind::Codex => codex::title_for(prompt, options, scratch_dir).await,
        AgentKind::Unknown => Err(AgentError::UnknownKind),
    }
}
