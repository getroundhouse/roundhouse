//! A process with a hub login is checked before anything can submit or replay a prompt.

use protocol::worker::AgentSpec;
use std::path::Path;
use tokio::process::{Child, ChildStdin};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use super::Tap;
use super::recovery::Recovery;
use crate::claude::login_check::{self, Output};
use crate::{AgentError, AgentOptions, AgentOutput};

pub(super) struct Launched {
    pub(super) child: Child,
    pub(super) pid: u32,
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
        let mut starting = Starting(Some(raw.pid));
        let mut stderr = JoinSet::new();
        stderr.spawn(crate::stdio::read_stderr(raw.stderr));
        let mut launched =
            Launched { child: raw.child, pid: raw.pid, stdin: raw.stdin, stdout: Output::new(raw.stdout), stderr };
        let checked = match &options.login {
            Some(login) => login_check::verify(&mut launched.stdin, &mut launched.stdout, login).await,
            None => Ok(()),
        };
        let error = match checked {
            Ok(()) => {
                starting.0 = None;
                return Ok(launched);
            }
            Err(error) => error,
        };
        process::signal_group(launched.pid, process::Signal::Kill);
        let _ = launched.child.wait().await;
        starting.0 = None;
        let stderr = super::supervise::drain_errors(launched.stderr).await;
        if let Some(corrected) = recovery.correct(&stderr, output).await {
            resume = corrected;
            continue;
        }
        return Err(error);
    }
}

/// Cancellation during verification stops the whole process group; Child reaps its CLI.
struct Starting(Option<u32>);
impl Drop for Starting {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            process::signal_group(pid, process::Signal::Kill);
        }
    }
}
