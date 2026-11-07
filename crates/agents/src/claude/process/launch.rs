//! Starting the CLI: its arguments, its environment, and finding the binary.

use std::path::PathBuf;
use std::process::Stdio;

use process::ProcessRecord;
use protocol::worker::AgentSpec;
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tracing::warn;

use crate::{AgentError, AgentOptions, BinaryLookup};

/// Set by a parent Claude session. If the hub was started from inside one, these would make a new
/// agent think it's part of that session. Login and provider settings such as `ANTHROPIC_API_KEY`
/// are the user's own choice, so they pass through.
pub const SCRUBBED_ENV: &[&str] = &[
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

pub fn build_args(spec: &AgentSpec, resume: bool) -> Vec<String> {
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
        // Lets full access be chosen later. It doesn't turn it on.
        "--allow-dangerously-skip-permissions",
    ]
    .map(String::from)
    .to_vec();
    args.push(if resume { "--resume" } else { "--session-id" }.to_string());
    args.push(spec.agent_session_id.to_string());
    // `default` is what the picker offers for the account's own default, and only the running CLI
    // has a name for it.
    if let Some(model) = spec.model.as_ref().filter(|model| *model != "default") {
        args.extend(["--model".to_string(), model.clone()]);
    }
    if let Some(mode) = &spec.permission_mode {
        args.extend(["--permission-mode".to_string(), mode.clone()]);
    }
    args
}

/// `PATH` first, then the places the installers use. A hub started from Finder may have neither
/// `~/.local/bin` nor Homebrew on its `PATH`, and it may not be able to read the login shell's.
pub(super) fn resolve_binary(lookup: &BinaryLookup) -> Option<PathBuf> {
    if let Some(path) = &lookup.explicit {
        return Some(path.clone());
    }
    let on_path = lookup.path.iter().flat_map(std::env::split_paths).map(|dir| dir.join("claude"));
    let installed =
        lookup.home.iter().flat_map(|home| [home.join(".local/bin/claude"), home.join(".claude/local/claude")]);
    let system = ["/opt/homebrew/bin/claude", "/usr/local/bin/claude"].map(PathBuf::from);
    on_path.chain(installed).chain(system).find(|path| is_executable(path))
}

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub(super) struct Launched {
    pub(super) child: Child,
    pub(super) pid: u32,
    pub(super) stdin: ChildStdin,
    pub(super) stdout: ChildStdout,
    pub(super) stderr: ChildStderr,
}

pub(super) fn launch(
    bin: &PathBuf,
    spec: &AgentSpec,
    resume: bool,
    options: &AgentOptions,
) -> Result<Launched, AgentError> {
    let mut command = Command::new(bin);
    command
        .args(build_args(spec, resume))
        .current_dir(&spec.cwd)
        // Reports idle, running and waiting directly. Counting `result` events can't, because a
        // prompt sent mid-turn folds into the running turn and produces no result of its own.
        .env("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Its own process group, so a signal to the hub's terminal doesn't reach it directly.
        .process_group(0)
        .kill_on_drop(true);
    for name in SCRUBBED_ENV.iter().chain(&protocol::paths::HUB_ONLY_ENV) {
        command.env_remove(name);
    }
    command.envs(spec.env.iter().map(|(k, v)| (k, v)));
    let mut child = command.spawn().map_err(|source| AgentError::Start {
        bin: bin.display().to_string(),
        cwd: spec.cwd.clone(),
        source,
    })?;
    let pid = child.id().unwrap_or_default();
    if let Some(path) = &options.pid_file
        && let Err(e) = ProcessRecord::write(path, pid)
    {
        warn!("could not record agent {pid} in {}: {e}", path.display());
    }
    let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(AgentError::NoPipes { bin: bin.display().to_string() });
    };
    Ok(Launched { child, pid, stdin, stdout, stderr })
}
