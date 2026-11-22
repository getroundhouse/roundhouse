//! Starting the CLI: its arguments, its environment, and finding the binary.

use std::path::{Path, PathBuf};

use process::Environment;
use protocol::model::AgentLogin;
use protocol::worker::AgentSpec;
use tokio::process::{Child, Command};

use super::Tap;
use crate::starting::Launched;
use crate::{AgentError, AgentOptions, BinaryLookup};

/// What a Claude session sets in the environment of what it starts. A CLI that inherits them takes itself for part of
/// that session. So a program that starts the CLI, and can itself be started from inside a session, removes them from
/// its own environment in its `main`, before it starts a thread, and nothing it starts has them: the daemon, for every
/// role it runs as, and the conformance example. This launch doesn't remove them again. Login and provider settings
/// such as `ANTHROPIC_API_KEY` are the user's own choice, so they pass through, unless the agent's hub hands it a login
/// to run under, as `claude::login` says.
pub const PARENT_SESSION_ENV: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_AGENT_SDK_VERSION",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
];

/// The CLI's arguments, which always name the model, the approval mode and the effort, so the CLI never picks any of
/// them for itself. A level this build has no name for is refused rather than left out, which would leave it to the
/// CLI.
pub fn build_args(spec: &AgentSpec, resume: bool) -> Result<Vec<String>, AgentError> {
    // Every launch has full access, even one for a chat that's planning, which `plan` asks into plan mode once it's
    // running.
    let crate::registry::Launch { model, mode: _, effort } = crate::registry::launch(spec)?;
    let level = crate::claude::effort::level(&effort).ok_or(AgentError::UnknownEffort)?;
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--thinking-display",
        "summarized",
        // Sends approvals to us as control requests. Without it, anything needing approval is denied.
        "--permission-prompt-tool",
        "stdio",
        // Lets full access be chosen again after the CLI has left it, and come back after a plan.
        "--allow-dangerously-skip-permissions",
        "--permission-mode",
        crate::claude::mode::FULL_ACCESS,
    ]
    .map(String::from)
    .to_vec();
    args.push(if resume { "--resume" } else { "--session-id" }.to_string());
    args.push(conversation(spec)?.to_string());
    args.extend(["--model".to_string(), model.as_str().to_string(), "--effort".to_string(), level.to_string()]);
    Ok(args)
}

pub(in crate::claude) fn resolve_binary(lookup: &BinaryLookup) -> Option<PathBuf> {
    crate::binary::resolve_binary("claude", &[".claude/local"], lookup)
}

pub(super) fn conversation(spec: &AgentSpec) -> Result<&protocol::model::AgentSessionId, AgentError> {
    use protocol::capability::conversation::Start;
    match &spec.start {
        Start::New { conversation: Some(id) } | Start::Resume { conversation: id } => Ok(id),
        Start::New { conversation: None } | Start::Fork { .. } | Start::Import { .. } => {
            Err(AgentError::Refused("Claude cannot start that conversation".into()))
        }
    }
}

/// Starts the CLI for `spec`, telling `tap` what it's started with.
pub(super) fn launch(
    bin: &Path,
    spec: &AgentSpec,
    resume: bool,
    options: &AgentOptions,
    tap: Option<&Tap>,
) -> Result<Launched, AgentError> {
    let cwd = Path::new(&spec.cwd);
    let mut command = crate::starting::piped(bin, cwd);
    let args = build_args(spec, resume)?;
    if let Some(tap) = tap {
        tap.launched(resume, Some(conversation(spec)?), &args);
    }
    command
        .args(args)
        // Reports idle, running and waiting directly. Counting `result` events can't, because a
        // prompt sent mid-turn folds into the running turn and produces no result of its own.
        .env("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS", "1")
        .envs(spec.env.iter().map(|(k, v)| (k, v)));
    if let Some(told) = &options.instructions {
        command.args(super::instructions::args(told)?);
    }
    if let Some(tools) = options.tools.as_ref().and_then(super::tools::args) {
        command.args(tools).env(super::tools::TAKES_TASKS, "1");
    }
    let child = start(&mut command, bin, cwd, options.login.as_ref(), &options.environment)?;
    Launched::new("Claude", bin, child, options.pid_file.as_deref())
}

/// Starts `command`, which runs `bin` in `cwd`, under `login` if its hub sent one, with what its worker gives every
/// process, and says which if it can't. The login goes on here, last but for that, so every start of the CLI has it
/// and nothing a caller set can undo it.
pub(in crate::claude) fn start(
    command: &mut Command,
    bin: &Path,
    cwd: &Path,
    login: Option<&AgentLogin>,
    environment: &Environment,
) -> Result<Child, AgentError> {
    crate::claude::login::run_under(command, login);
    crate::starting::start(command, bin, cwd, environment)
}
