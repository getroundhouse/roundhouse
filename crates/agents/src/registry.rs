use crate::{AgentError, AgentOptions, AgentOutput, AgentProcess, antigravity, claude, codex};
use protocol::attachment::NewAttachment;
use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings::{Effort, Settings};
use protocol::model::{AgentAccount, AgentDescription, AgentKind, ModelName, Title};
use protocol::prompt::Prompt;
use protocol::worker::AgentSpec;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// What each kind of agent this build runs is called and offers, as its adapter describes it, for the hub to give each
/// chat what its agent starts with and to tell clients what to offer.
pub fn descriptions() -> Vec<AgentDescription> {
    vec![claude::description(), codex::description(), antigravity::description()]
}

pub async fn spawn(
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    let description = described(spec.agent)?;
    let claims = description.capabilities.as_ref().ok_or(AgentError::UnknownKind)?;
    if !spec.start.allowed(claims) || !spec.settings.allowed(claims) {
        return Err(AgentError::Refused("The agent does not offer these launch settings".into()));
    }
    match spec.agent {
        AgentKind::Claude => claude::spawn(spec, output, options).await,
        AgentKind::Codex => codex::spawn(spec, output, options).await,
        AgentKind::Antigravity => antigravity::spawn(spec, output, options).await,
        AgentKind::Unknown => Err(AgentError::UnknownKind),
    }
}

/// What a launch runs its agent with, as [`launch`] reads it from the launch's typed settings.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Launch {
    pub(crate) model: ModelName,
    pub(crate) mode: ApprovalMode,
    pub(crate) effort: Effort,
}

/// What `spec` runs its agent with: the model it names, which every launch does, and the mode and effort it names, or
/// its kind's, where it leaves them out, as a one-off call does. A mode or an effort this build doesn't know is
/// refused, since the adapter couldn't tell its agent. Which mode the kind offers the hub checked, as `spawn` does.
pub(crate) fn launch(spec: &AgentSpec) -> Result<Launch, AgentError> {
    launching(&described(spec.agent)?, &spec.settings)
}

/// What `kind` runs with where nothing names a setting, as a connection to its CLI that starts no conversation does.
pub(crate) fn defaults(kind: AgentKind) -> Result<Launch, AgentError> {
    let description = described(kind)?;
    launching(&description, &description.defaults)
}

fn launching(description: &AgentDescription, settings: &Settings) -> Result<Launch, AgentError> {
    let model = settings.model.clone().ok_or_else(|| AgentError::Refused("A launch names its model".into()))?;
    let mode = settings.mode.or_else(|| description.default_mode());
    let mode = mode.filter(|mode| mode.is_known()).ok_or(AgentError::UnknownMode)?;
    let effort = settings.effort.as_ref().or(description.defaults.effort.as_ref()).ok_or(AgentError::UnknownEffort)?;
    Ok(Launch { model, mode, effort: effort.known().ok_or(AgentError::UnknownEffort)? })
}

fn described(kind: AgentKind) -> Result<AgentDescription, AgentError> {
    descriptions().into_iter().find(|description| description.kind == kind).ok_or(AgentError::UnknownKind)
}

/// The account `kind`'s CLI runs under here, with `options.login` or with this machine's own, from a one-off call
/// that runs in `scratch_dir`.
pub async fn account(kind: AgentKind, options: &AgentOptions, scratch_dir: &Path) -> Result<AgentAccount, AgentError> {
    match kind {
        AgentKind::Claude => claude::account(options, scratch_dir).await,
        AgentKind::Codex => codex::account(options, scratch_dir).await,
        AgentKind::Antigravity => antigravity::account(options, scratch_dir).await,
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
        AgentKind::Antigravity => antigravity::title_for(prompt, options, scratch_dir).await,
        AgentKind::Unknown => Err(AgentError::UnknownKind),
    }
}

#[cfg(test)]
mod tests {
    use protocol::capability::approvals::{ApprovalMode, Unasked};
    use protocol::capability::conversation::Start;
    use protocol::capability::settings::{Effort, Settings};
    use protocol::model::{AgentKind, ModelName, RunId};
    use protocol::worker::AgentSpec;

    fn launching(settings: Settings) -> AgentSpec {
        AgentSpec {
            session_id: "s".into(),
            run: RunId::new(1),
            agent: AgentKind::Claude,
            cwd: "/w".into(),
            start: Start::New { conversation: None },
            settings,
            env: Vec::new(),
            tools_token: None,
            login: None,
        }
    }

    /// A launch runs its agent with the model it names and the mode and effort it names, or the kind's where it names
    /// none, and with nothing this build doesn't know.
    #[test]
    fn a_launch_runs_with_its_model_and_what_it_names_or_the_kinds() {
        let opus = ModelName::named("opus");
        let claude = super::descriptions().into_iter().find(|d| d.kind == AgentKind::Claude).unwrap();
        let modes = claude.capabilities.as_ref().unwrap().approvals.modes.clone().unwrap();
        let planning = modes.others[0];
        let named = Settings {
            model: Some(opus.clone()),
            mode: Some(planning),
            effort: Some(Effort::Max),
            ..Settings::default()
        };

        let launch = super::launch(&launching(named.clone())).unwrap();
        assert_eq!((launch.model, launch.mode, launch.effort), (opus.clone(), planning, Effort::Max));
        let only_a_model = super::launch(&launching(Settings { model: Some(opus), ..Settings::default() })).unwrap();
        let kinds = (modes.default, claude.defaults.effort.unwrap());
        assert_eq!((only_a_model.mode, only_a_model.effort), kinds, "the kind's");
        let unnamed = Settings { effort: Some(Effort::Persistent), ..named.clone() };
        assert_eq!(super::launch(&launching(unnamed)).unwrap().effort, Effort::Persistent, "one it has no level for");

        assert!(super::launch(&launching(Settings { model: None, ..named.clone() })).is_err(), "no model");
        assert!(super::launch(&launching(Settings { effort: Some(Effort::Unknown), ..named.clone() })).is_err());
        let newer = ApprovalMode { unasked: Unasked::Unknown, ..modes.default };
        assert!(super::launch(&launching(Settings { mode: Some(newer), ..named })).is_err(), "a newer mode");
    }

    #[test]
    fn every_kind_claims_only_what_it_can_be_held_to() {
        for description in super::descriptions() {
            let claims = description.capabilities.unwrap_or_default();
            assert_eq!(claims.unmet_needs(), [], "{:?}", description.kind);
        }
    }

    /// A kind claims its modes and either offers its defaults or gets its model choices from the launch, and its
    /// defaults name a model and an effort and leave the mode to its claims.
    #[test]
    fn a_kind_declares_its_choices_or_gets_its_catalog_at_launch() {
        for agent in super::descriptions() {
            let model = agent.defaults.model.clone().unwrap();
            let effort = agent.defaults.effort.clone().unwrap();

            if agent.discovers_models() {
                assert!(agent.models.is_empty() && agent.efforts.is_empty(), "{:?} discovers both", agent.kind);
            } else {
                assert!(agent.label(&model).is_some(), "{:?}'s model", agent.kind);
                assert!(agent.efforts_for(&model).contains(&effort), "{:?}'s effort", agent.kind);
            }
            assert!(agent.default_mode().is_some_and(ApprovalMode::is_known), "{:?}'s mode", agent.kind);
            assert_eq!(agent.defaults.mode, None, "{:?}'s mode is its claims'", agent.kind);
        }
    }
}
