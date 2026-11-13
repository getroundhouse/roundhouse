use crate::conformance::scenario::Scenario;
use protocol::model::{AgentKind, AgentSessionId, Effort, ModelName, RunId, SessionSettings};
use protocol::worker::AgentSpec;
use std::path::Path;

/// The cheapest model, by its full ID, since the CLI answers a chat in plan mode on the alias `haiku` with Sonnet, as
/// `docs/claude-cli.md` says. The scripted model answers to any name, and says it's the one asked for.
pub(super) const MODEL: ModelName = ModelName::named("claude-haiku-5-5");

/// What a scenario's process for `run` is launched with: the chat's mode, the cheapest model, and the least effort.
pub(super) fn spec(
    scenario: &Scenario,
    run: RunId,
    cwd: &Path,
    session: &AgentSessionId,
    resume: bool,
    env: Vec<(String, String)>,
) -> AgentSpec {
    let settings = SessionSettings { model: MODEL, permission_mode: scenario.mode, effort: Effort::Low };
    AgentSpec {
        session_id: "conformance".into(),
        run,
        agent: AgentKind::Claude,
        cwd: cwd.to_string_lossy().into_owned(),
        agent_session_id: session.clone(),
        resume,
        settings,
        env,
        tools_token: None,
        login: None,
    }
}

/// What Claude names the folder it keeps a project's conversations in: its path, with every character but a letter
/// or digit made `-`.
pub(super) fn project_folder(path: &str) -> String {
    path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}
