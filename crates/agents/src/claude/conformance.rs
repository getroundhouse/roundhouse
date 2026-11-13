//! Claude's binding for [`crate::conformance`]: the scripted model's API and each scenario's script, how a scenario's
//! processes are launched on the real CLI, and how a capture of them is replayed through the adapter.
//! `docs/conformance.md` says what a trial is for, and how to play one.

use std::path::Path;

use protocol::model::{AgentKind, AgentSessionId, Effort, ModelName, RunId, SessionSettings};
use protocol::worker::AgentSpec;

use crate::conformance::scenario::Scenario;

mod fixture;
mod live;
pub mod messages;
mod replay;
mod scripts;
mod stream_json;

pub use live::{KEPT_ENV, Live, Options, find_cli, live};
pub use replay::replay;

/// The cheapest model, by its full ID, since the CLI answers a chat in plan mode on the alias `haiku` with Sonnet, as
/// `docs/claude-cli.md` says. The scripted model answers to any name, and says it's the one asked for.
const MODEL: ModelName = ModelName::named("claude-haiku-5-5");

/// What a scenario's process for `run` is launched with: the chat's mode, the cheapest model, and the least effort.
fn spec(
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
fn project_folder(path: &str) -> String {
    path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}
