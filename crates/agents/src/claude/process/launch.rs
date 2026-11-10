//! Starting the CLI: its arguments, its environment, and finding the binary.

use std::path::{Path, PathBuf};
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
    if let Some(model) = spec.options.model.as_ref().filter(|model| *model != "default") {
        args.extend(["--model".to_string(), model.clone()]);
    }
    if let Some(mode) = &spec.options.permission_mode {
        args.extend(["--permission-mode".to_string(), mode.clone()]);
    }
    // A level this build doesn't know is left out, which the CLI would ignore anyway, with a warning.
    if let Some(level) = spec.options.effort.and_then(crate::claude::effort::level) {
        args.extend(["--effort".to_string(), level.to_string()]);
    }
    args
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
        .args(build_args(spec, resume))
        // Reports idle, running and waiting directly. Counting `result` events can't, because a
        // prompt sent mid-turn folds into the running turn and produces no result of its own.
        .env("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS", "1")
        .envs(spec.env.iter().map(|(k, v)| (k, v)));
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
