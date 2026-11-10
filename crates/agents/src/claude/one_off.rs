//! One call to the CLI that isn't a session: start it, give it what to read, and wait for what it prints. A call cut
//! short takes everything it started with it.

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use process::{Signal, signal_group};
use tokio::io::AsyncWriteExt as _;

use super::process::{command, record, start};
use crate::{AgentError, CallFailed};

/// What to run, and how long to give it.
pub(super) struct OneOff<'a> {
    pub(super) bin: &'a Path,
    /// Where it runs, which decides whose project settings it reads.
    pub(super) cwd: &'a Path,
    pub(super) args: Vec<String>,
    /// What it reads on stdin, which is then closed.
    pub(super) stdin: &'a [u8],
    /// A call still going after this long is stuck.
    pub(super) timeout: Duration,
    /// Where to record it while it runs, so the provider can stop it if the worker is killed.
    pub(super) pid_file: Option<PathBuf>,
}

/// Runs the call and answers with what it printed and how it exited, which is for the caller to judge. Dropping the
/// future stops the call and everything it started.
pub(super) async fn run(call: OneOff<'_>) -> Result<Output, AgentError> {
    let mut command = command(call.bin, call.cwd);
    command.args(call.args);
    let mut child = start(&mut command, call.bin, call.cwd)?;
    let running = Running::new(child.id().unwrap_or_default(), call.pid_file);
    let mut stdin = child.stdin.take().ok_or_else(|| AgentError::NoPipes { bin: call.bin.display().to_string() })?;
    stdin.write_all(call.stdin).await.map_err(CallFailed::Io)?;
    drop(stdin);
    let output = tokio::time::timeout(call.timeout, child.wait_with_output())
        .await
        .map_err(|_| CallFailed::TimedOut(call.timeout))?
        .map_err(CallFailed::Io)?;
    running.finished();
    Ok(output)
}

/// A call under way, recorded so the provider can stop it if the worker is killed. Unless it finished, dropping this
/// stops its whole process group, which `kill_on_drop` alone doesn't: that reaches only the CLI.
struct Running {
    pid: u32,
    record: Option<PathBuf>,
    finished: bool,
}

impl Running {
    fn new(pid: u32, record_at: Option<PathBuf>) -> Self {
        record(record_at.as_deref(), pid);
        Running { pid, record: record_at, finished: false }
    }

    /// The CLI has exited and been waited for, so its group may be gone and its PID reused.
    fn finished(mut self) {
        self.finished = true;
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if !self.finished {
            signal_group(self.pid, Signal::Kill);
        }
        if let Some(path) = &self.record {
            let _ = std::fs::remove_file(path);
        }
    }
}
