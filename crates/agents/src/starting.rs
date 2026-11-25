//! Starting an agent's CLI, for a session or a one-off call, which every kind of agent does here, so each start has
//! what its worker gives every process it starts.

use std::path::Path;
use std::process::Stdio;

use process::Environment;
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tracing::warn;

use crate::{AgentError, AgentOptions};

/// Starts `command`, which runs `bin` in `cwd`, with `environment` on it last, so nothing the launch set can undo it,
/// and says which if it can't.
pub(crate) fn start(
    command: &mut Command,
    bin: &Path,
    cwd: &Path,
    environment: &Environment,
) -> Result<Child, AgentError> {
    environment.apply(command.as_std_mut());
    command.spawn().map_err(|source| AgentError::Start {
        bin: bin.display().to_string(),
        cwd: cwd.display().to_string(),
        source,
    })
}

/// A CLI that [`start_piped`] started, with its stdio.
pub(crate) struct Launched {
    pub(crate) child: Child,
    pub(crate) pid: u32,
    pub(crate) stdin: ChildStdin,
    pub(crate) stdout: ChildStdout,
    pub(crate) stderr: ChildStderr,
}

impl Launched {
    /// `child`, which `peer`'s CLI at `bin` was started as from a [`piped`] command, recorded at `pid_file`, so the
    /// provider can stop it if the worker is killed. Failing to record it only loses that, so it's logged.
    pub(crate) fn new(peer: &str, bin: &Path, mut child: Child, pid_file: Option<&Path>) -> Result<Self, AgentError> {
        let pid = child.id().unwrap_or_default();
        if let Some(path) = pid_file
            && let Err(error) = process::ProcessRecord::write(path, pid)
        {
            warn!(
                name: "agent.process_unrecorded",
                peer,
                pid,
                path = %path.display(),
                %error,
                "Couldn't record a process, so it's left running if the worker is killed"
            );
        }
        let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(AgentError::NoPipes { bin: bin.display().to_string() });
        };
        Ok(Launched { child, pid, stdin, stdout, stderr })
    }
}

/// The CLI at `bin`, run in `cwd` with its stdio piped. It's in a process group of its own, so a stop reaches
/// everything it starts and a signal to the hub's terminal doesn't reach it directly.
pub(crate) fn piped(bin: &Path, cwd: &Path) -> Command {
    let mut command = Command::new(bin);
    command.current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    process::own_group(command.as_std_mut());
    command
}

/// Starts `peer`'s CLI at `bin`, with `args` and `env`, in `cwd`, as [`piped`] and [`Launched::new`] say. `peer` runs
/// under the login on its worker's machine, so a login in `options` is refused.
pub(crate) fn start_piped(
    peer: &'static str,
    bin: &Path,
    args: &[&str],
    cwd: &Path,
    env: &[(String, String)],
    options: &AgentOptions,
) -> Result<Launched, AgentError> {
    if options.login.is_some() {
        return Err(AgentError::Refused(format!("{peer} uses the login on its worker's machine")));
    }
    let mut command = piped(bin, cwd);
    command.args(args).envs(env.iter().map(|(key, value)| (key, value)));
    let child = start(&mut command, bin, cwd, &options.environment)?;
    Launched::new(peer, bin, child, options.pid_file.as_deref())
}
