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

/// Starts `peer`'s CLI at `bin`, with `args` and `env`, in `cwd`, its stdio piped, in a process group of its own, so a
/// stop reaches everything it starts. It's recorded at `options.pid_file`, so the provider can stop it if the worker is
/// killed, and failing to record it only loses that, so it's logged. `peer` runs under the login on its worker's
/// machine, so a login in `options` is refused.
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
    let mut command = Command::new(bin);
    command
        .args(args)
        .current_dir(cwd)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    process::own_group(command.as_std_mut());
    let mut child = start(&mut command, bin, cwd, &options.environment)?;
    let pid = child.id().unwrap_or_default();
    if let Some(path) = &options.pid_file
        && let Err(error) = process::ProcessRecord::write(path, pid)
    {
        warn!("could not record {peer} {pid} in {}: {error}", path.display());
    }
    let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(AgentError::NoPipes { bin: bin.display().to_string() });
    };
    Ok(Launched { child, pid, stdin, stdout, stderr })
}
