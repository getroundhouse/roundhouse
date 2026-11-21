//! A process with a hub login is checked before anything can submit or replay a prompt.

use protocol::worker::AgentSpec;
use std::path::Path;
use tokio::process::{Child, ChildStdin};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use super::Tap;
use super::recovery::Recovery;
use crate::claude::login_check::{self, Output};
use crate::stdio::GroupGuard;
use crate::{AgentError, AgentOptions, AgentOutput};

pub(super) struct Launched {
    pub(super) child: Child,
    pub(super) pid: u32,
    /// Kills the CLI's group if it's dropped before the run holds its pid, which it's [`GroupGuard::finished`] once
    /// it does, so a start that's cancelled or stopped leaves nothing running.
    pub(super) group: GroupGuard,
    pub(super) stdin: ChildStdin,
    pub(super) stdout: Output,
    pub(super) stderr: JoinSet<Vec<String>>,
}

pub(super) async fn launch(
    bin: &Path,
    spec: &AgentSpec,
    mut resume: bool,
    options: &AgentOptions,
    tap: Option<&Tap>,
    output: &mpsc::Sender<AgentOutput>,
    recovery: &mut Recovery,
) -> Result<Launched, AgentError> {
    loop {
        let raw = super::launch::launch(bin, spec, resume, options, tap)?;
        let mut stderr = JoinSet::new();
        stderr.spawn(crate::stdio::read_stderr(raw.stderr));
        let mut launched = Launched {
            child: raw.child,
            pid: raw.pid,
            // With no record, which a guard removes as it's dropped, since the run's lasts until the run ends, when
            // `RunFiles` removes it.
            group: GroupGuard::new(raw.pid, None),
            stdin: raw.stdin,
            stdout: Output::new(raw.stdout),
            stderr,
        };
        let checked = match &options.login {
            Some(login) => login_check::verify(&mut launched.stdin, &mut launched.stdout, login).await,
            None => Ok(()),
        };
        let error = match checked {
            Ok(()) => return Ok(launched),
            Err(error) => error,
        };
        drop(launched.group);
        let _ = launched.child.wait().await;
        let stderr = super::supervise::drain_errors(launched.stderr).await;
        if let Some(corrected) = recovery.correct(&stderr, output).await {
            resume = corrected;
            continue;
        }
        return Err(error);
    }
}
