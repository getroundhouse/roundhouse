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
    // Written while what it prints is read, so neither waits on the other, however much there is of each.
    let writing = async move {
        let written = stdin.write_all(call.stdin).await;
        drop(stdin);
        written
    };
    let (written, output) =
        tokio::time::timeout(call.timeout, async { tokio::join!(writing, child.wait_with_output()) })
            .await
            .map_err(|_| CallFailed::TimedOut(call.timeout))?;
    let output = output.map_err(CallFailed::Io)?;
    running.finished();
    // A CLI that stops reading, such as one refusing its flags, says why itself, which a broken pipe doesn't.
    if output.status.success() {
        written.map_err(CallFailed::Io)?;
    }
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// More than a pipe holds, so a write that waits for the other end to read would wait forever.
    const MORE_THAN_A_PIPE: usize = 1024 * 1024;

    async fn shell(script: &str, stdin: &[u8]) -> Result<Output, AgentError> {
        let args = vec!["-c".to_string(), script.to_string()];
        let timeout = Duration::from_secs(20);
        run(OneOff { bin: Path::new("/bin/sh"), cwd: Path::new("/"), args, stdin, timeout, pid_file: None }).await
    }

    /// Such as a CLI that prints its first lines before it has read all of a prompt with a big image in it.
    #[tokio::test]
    async fn a_call_that_prints_a_lot_before_it_reads_gets_all_it_was_given() {
        let script = format!("head -c {MORE_THAN_A_PIPE} /dev/zero; wc -c | tr -d ' '");

        let output = shell(&script, &vec![b'x'; MORE_THAN_A_PIPE]).await.unwrap();

        assert!(output.status.success());
        let (printed, counted) = output.stdout.split_at(MORE_THAN_A_PIPE);
        assert!(printed.iter().all(|byte| *byte == 0));
        assert_eq!(String::from_utf8_lossy(counted).trim(), MORE_THAN_A_PIPE.to_string(), "every byte of stdin");
    }

    /// Such as a CLI refusing its flags before it reads anything, which says why, where a broken pipe wouldn't.
    #[tokio::test]
    async fn a_call_that_fails_without_reading_says_why() {
        let output = shell("echo 'unknown option' >&2; exit 1", &vec![b'x'; MORE_THAN_A_PIPE]).await.unwrap();

        assert!(!output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), "unknown option");
    }
}
