//! Starting the CLI: its arguments, its environment, and finding the binary.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use process::ProcessRecord;
use protocol::model::SessionSettings;
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

/// The CLI's arguments, which always name the model, the approval mode and the effort, so the CLI never picks any of
/// them for itself. A level this build has no name for is refused rather than left out, which would leave it to the
/// CLI.
pub fn build_args(spec: &AgentSpec, resume: bool) -> Result<Vec<String>, AgentError> {
    // Every launch has full access, even one for a chat that's planning, which `plan` asks into plan mode once it's
    // running, or one for a chat in a mode of the CLI's own, which can't be chosen.
    let SessionSettings { model, permission_mode: _, effort } = &spec.settings;
    let level = crate::claude::effort::level(*effort).ok_or(AgentError::UnknownEffort)?;
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
    args.push(spec.agent_session_id.to_string());
    args.extend(["--model".to_string(), model.as_str().to_string(), "--effort".to_string(), level.to_string()]);
    Ok(args)
}

/// `PATH` first, then the places the installers use. A hub started from Finder may have neither
/// `~/.local/bin` nor Homebrew on its `PATH`, and it may not be able to read the login shell's.
pub(in crate::claude) fn resolve_binary(lookup: &BinaryLookup) -> Option<PathBuf> {
    if let Some(path) = &lookup.explicit {
        return Some(path.clone());
    }
    // On Windows that's the program its installer puts in place, and not the script npm installs.
    let file = format!("claude{}", std::env::consts::EXE_SUFFIX);
    let on_path = lookup.path.iter().flat_map(std::env::split_paths).map(|dir| dir.join(&file));
    let installed = lookup
        .home
        .iter()
        .flat_map(|home| [home.join(".local/bin").join(&file), home.join(".claude/local").join(&file)]);
    let system = ["/opt/homebrew/bin/claude", "/usr/local/bin/claude"].map(PathBuf::from);
    on_path.chain(installed).chain(system).find(|path| is_executable(path))
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Windows goes by a file's extension, which the names looked for already have.
#[cfg(windows)]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

pub(super) struct Launched {
    pub(super) child: Child,
    pub(super) pid: u32,
    pub(super) stdin: ChildStdin,
    pub(super) stdout: ChildStdout,
    pub(super) stderr: ChildStderr,
}

pub(super) fn launch(
    bin: &Path,
    spec: &AgentSpec,
    resume: bool,
    options: &AgentOptions,
) -> Result<Launched, AgentError> {
    let cwd = Path::new(&spec.cwd);
    let mut command = command(bin, cwd);
    command
        .args(build_args(spec, resume)?)
        // Reports idle, running and waiting directly. Counting `result` events can't, because a
        // prompt sent mid-turn folds into the running turn and produces no result of its own.
        .env("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS", "1")
        .envs(spec.env.iter().map(|(k, v)| (k, v)));
    if let Some(tools) = options.tools.as_ref().and_then(super::tools::args) {
        command.args(tools).env(super::tools::TAKES_TASKS, "1");
    }
    let mut child = start(&mut command, bin, cwd)?;
    let pid = child.id().unwrap_or_default();
    record(options.pid_file.as_deref(), pid);
    let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(AgentError::NoPipes { bin: bin.display().to_string() });
    };
    Ok(Launched { child, pid, stdin, stdout, stderr })
}

/// The CLI at `bin`, run in `cwd` with its stdio piped. It's in a process group of its own, so a stop reaches
/// everything it starts and a signal to the hub's terminal doesn't reach it directly, and it doesn't get what a
/// parent Claude session set.
pub(in crate::claude) fn command(bin: &Path, cwd: &Path) -> Command {
    let mut command = Command::new(bin);
    command.current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    process::own_group(command.as_std_mut());
    for name in SCRUBBED_ENV {
        command.env_remove(name);
    }
    command
}

/// Starts `command`, which runs `bin` in `cwd`, and says which if it can't.
pub(in crate::claude) fn start(command: &mut Command, bin: &Path, cwd: &Path) -> Result<Child, AgentError> {
    command.spawn().map_err(|source| AgentError::Start {
        bin: bin.display().to_string(),
        cwd: cwd.display().to_string(),
        source,
    })
}

/// Records a running process at `pid_file`, so the provider can stop it if this worker is killed. Failing to only
/// loses that, so it's logged rather than stopping the process.
pub(in crate::claude) fn record(pid_file: Option<&Path>, pid: u32) {
    if let Some(path) = pid_file
        && let Err(e) = ProcessRecord::write(path, pid)
    {
        warn!("could not record {pid} in {}: {e}", path.display());
    }
}
