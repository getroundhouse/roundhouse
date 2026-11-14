use crate::{AgentError, AgentOptions, AgentOutput, AgentProcess, claude, codex};
use protocol::attachment::NewAttachment;
use protocol::model::{AgentAccount, AgentDescription, AgentKind, NotOffered, SessionSettings, Title};
use protocol::prompt::Prompt;
use protocol::worker::AgentSpec;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// What each kind of agent this build runs is called and offers, as its adapter describes it, for the hub to give each
/// chat what its agent starts with and to tell clients what to offer.
pub fn descriptions() -> Vec<AgentDescription> {
    vec![claude::description(), codex::description()]
}

pub async fn spawn(
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    let description = descriptions().into_iter().find(|d| d.kind == spec.agent).ok_or(AgentError::UnknownKind)?;
    let claims = description.capabilities.as_ref().ok_or(AgentError::UnknownKind)?;
    if !spec.start.allowed(claims) || !spec.settings.allowed(claims) {
        return Err(AgentError::Refused("The agent does not offer these launch settings".into()));
    }
    match spec.agent {
        AgentKind::Claude => claude::spawn(spec, output, options).await,
        AgentKind::Codex => codex::spawn(spec, output, options).await,
        AgentKind::Unknown => Err(AgentError::UnknownKind),
    }
}

/// What `spec` runs its agent with, in the form the adapters run their CLIs with, with what it leaves out taken from its
/// kind's defaults. A bridge, until the adapters take a launch's typed settings themselves in #1274, which goes then, so
/// a new adapter takes `spec.settings` rather than this.
pub(crate) fn settings_of(spec: &AgentSpec) -> Result<SessionSettings, AgentError> {
    let defaults = descriptions().into_iter().find(|d| d.kind == spec.agent).ok_or(AgentError::UnknownKind)?.defaults;
    SessionSettings::from_typed(&spec.settings, &defaults).map_err(|refused| match refused {
        NotOffered::Effort => AgentError::UnknownEffort,
        NotOffered::Mode => AgentError::UnknownMode,
        NotOffered::Settings => AgentError::Refused("A launch names the model its agent runs on".into()),
    })
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

#[cfg(test)]
mod tests {
    #[test]
    fn every_kind_claims_only_what_it_can_be_held_to() {
        for description in super::descriptions() {
            let claims = description.capabilities.unwrap_or_default();
            assert_eq!(claims.unmet_needs(), [], "{:?}", description.kind);
        }
    }
}
