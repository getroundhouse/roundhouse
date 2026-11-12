//! Runs the command a provider's `connect` answers with, whose stdin and stdout are the worker's stream, and ends
//! it. The hub runs it rather than the provider, so a provider is never in a worker's stream, and ending the
//! connection works the same whatever the provider.

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use providers::WorkerCommand;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::process::{Child, Command};
use tokio::sync::{Notify, watch};
use tracing::{info, warn};

/// How long a worker gets to stop its agents after its stdin closes. Agents get 3 s each.
pub(crate) const STOP_GRACE: Duration = Duration::from_secs(10);

/// The running command. Dropping this leaves it running, and the task watching it reaps it.
pub(crate) struct WorkerProcess {
    kill: Arc<Notify>,
    exited: watch::Receiver<bool>,
}

pub(crate) struct Spawned {
    pub process: WorkerProcess,
    pub reader: Box<dyn AsyncRead + Send + Unpin>,
    pub writer: Box<dyn AsyncWrite + Send + Unpin>,
}

/// Starts `command`, with what it writes to stderr appended to `log`.
pub(crate) fn spawn(command: &WorkerCommand, log: &Path) -> Result<Spawned, ApiError> {
    let failed = |what: String, e: std::io::Error| ApiError::new(ErrorCode::Worker, format!("{what}: {e}"));
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir).map_err(|e| failed(format!("creating {}", dir.display()), e))?;
    }
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| failed(format!("opening {}", log.display()), e))?;
    let mut child = Command::new(&command.program);
    child
        .args(&command.args)
        .envs(command.env.iter().map(|(name, value)| (name, value)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log_file))
        .kill_on_drop(true);
    // So Ctrl-C on a foreground hub goes through the hub's clean shutdown.
    process::own_group(child.as_std_mut());
    let mut child = child.spawn().map_err(|e| failed(format!("starting {}", command.program), e))?;
    let (Some(writer), Some(reader)) = (child.stdin.take(), child.stdout.take()) else {
        return Err(ApiError::new(ErrorCode::Worker, format!("{} started without its pipes", command.program)));
    };
    info!(pid = child.id(), program = command.program, "started a worker");
    let kill = Arc::new(Notify::new());
    let (exited_tx, exited) = watch::channel(false);
    tokio::spawn(watch_child(child, kill.clone(), exited_tx));
    Ok(Spawned { process: WorkerProcess { kill, exited }, reader: Box::new(reader), writer: Box::new(writer) })
}

async fn watch_child(mut child: Child, kill: Arc<Notify>, exited: watch::Sender<bool>) {
    tokio::select! {
        _ = child.wait() => {}
        () = kill.notified() => {
            // The whole group it leads, so nothing the command started outlives it. A worker's agents and scripts lead
            // groups of their own, which its provider ends.
            match child.id() {
                Some(pid) => process::signal_group(pid, process::Signal::Kill),
                None => drop(child.start_kill()),
            }
            let _ = child.wait().await;
        }
    }
    // With `send_replace`, because nobody may be waiting yet.
    exited.send_replace(true);
}

impl WorkerProcess {
    /// Ends it at once, without waiting.
    pub(crate) fn kill(&self) {
        self.kill.notify_one();
    }

    /// Waits up to `grace` for it to exit, as it does once its stdin has closed, and ends it if it hasn't.
    pub(crate) async fn end(&self, grace: Duration) {
        let mut exited = self.exited.clone();
        if tokio::time::timeout(grace, exited.wait_for(|done| *done)).await.is_err() {
            warn!("a worker didn't stop in time, killing it");
            self.kill();
            let _ = exited.wait_for(|done| *done).await;
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::time::Instant;

    use tokio::io::{AsyncBufReadExt, BufReader};

    use super::*;

    fn sh(dir: &Path, script: &str) -> Spawned {
        let command = WorkerCommand { program: "sh".into(), args: vec!["-c".into(), script.into()], env: Vec::new() };
        spawn(&command, &dir.join("logs/worker.log")).unwrap()
    }

    #[tokio::test]
    async fn ending_waits_for_a_command_that_exits_once_its_stdin_closes() {
        let dir = tempfile::tempdir().unwrap();
        let Spawned { process: command, reader, writer } = sh(dir.path(), "cat; echo stopped >&2");
        drop(writer);
        let started = Instant::now();

        command.end(STOP_GRACE).await;

        assert!(started.elapsed() < STOP_GRACE / 2, "it should have exited by itself");
        drop(reader);
        let log = std::fs::read_to_string(dir.path().join("logs/worker.log")).unwrap();
        assert_eq!(log, "stopped\n", "what it writes to stderr goes to its log");
    }

    #[tokio::test]
    async fn ending_one_that_keeps_running_ends_its_whole_group() {
        let dir = tempfile::tempdir().unwrap();
        let Spawned { process: command, reader, writer: _writer } = sh(dir.path(), "sleep 100 & echo $!; wait");
        let mut line = String::new();
        BufReader::new(reader).read_line(&mut line).await.unwrap();
        let helper: u32 = line.trim().parse().unwrap();
        assert!(process::testing::is_running(helper));

        command.end(Duration::from_millis(100)).await;

        let deadline = Instant::now() + Duration::from_secs(5);
        while process::testing::is_running(helper) {
            assert!(Instant::now() < deadline, "what the command started outlived it");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}
