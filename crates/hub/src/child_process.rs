//! Runs a child of the hub whose stdin and stdout are a stream, and ends it: a worker's command, which its provider's
//! `connect` answers with, or a provider's adapter. The hub runs a worker's command rather than its provider, so a
//! provider is never in a worker's stream, and ending a connection works the same whatever the provider.

use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::process::{Child, Command};
use tokio::sync::{Notify, watch};
use tracing::{info, warn};

/// The running child. Dropping this leaves it running, and the task watching it reaps it.
pub(crate) struct ChildProcess {
    program: String,
    kill: Arc<Notify>,
    exited: watch::Receiver<bool>,
}

pub(crate) struct Spawned {
    pub process: ChildProcess,
    pub reader: Box<dyn AsyncRead + Send + Unpin>,
    pub writer: Box<dyn AsyncWrite + Send + Unpin>,
}

/// Where what a child writes to stderr goes.
#[derive(Clone, Copy)]
pub(crate) enum Stderr<'a> {
    /// Appended to this file, which is made if need be.
    Log(&'a Path),
    /// The hub's own.
    Hubs,
}

/// Starts `program` as the leader of a process group of its own, so Ctrl-C on a foreground hub goes through the hub's
/// clean shutdown, and ending it ends whatever it started.
pub(crate) fn spawn(
    program: &str,
    args: &[String],
    env: &[(String, String)],
    stderr: Stderr<'_>,
) -> io::Result<Spawned> {
    let context = |what: String| move |e: io::Error| io::Error::new(e.kind(), format!("{what}: {e}"));
    let stderr = match stderr {
        Stderr::Log(log) => {
            if let Some(dir) = log.parent() {
                std::fs::create_dir_all(dir).map_err(context(format!("creating {}", dir.display())))?;
            }
            let file = std::fs::OpenOptions::new().create(true).append(true).open(log);
            Stdio::from(file.map_err(context(format!("opening {}", log.display())))?)
        }
        Stderr::Hubs => Stdio::inherit(),
    };
    let mut child = Command::new(program);
    child
        .args(args)
        .envs(env.iter().map(|(name, value)| (name, value)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(stderr)
        .kill_on_drop(true);
    process::own_group(child.as_std_mut());
    let mut child = child.spawn().map_err(context(format!("starting {program}")))?;
    let (Some(writer), Some(reader)) = (child.stdin.take(), child.stdout.take()) else {
        return Err(io::Error::other(format!("{program} started without its pipes")));
    };
    info!(pid = child.id(), program, "started a child of the hub");
    let kill = Arc::new(Notify::new());
    let (exited_tx, exited) = watch::channel(false);
    tokio::spawn(watch_child(child, kill.clone(), exited_tx));
    let process = ChildProcess { program: program.to_string(), kill, exited };
    Ok(Spawned { process, reader: Box::new(reader), writer: Box::new(writer) })
}

async fn watch_child(mut child: Child, kill: Arc<Notify>, exited: watch::Sender<bool>) {
    tokio::select! {
        _ = child.wait() => {}
        () = kill.notified() => {
            // The whole group it leads, so nothing it started outlives it. A worker's agents and scripts lead groups
            // of their own, which its provider ends.
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

impl ChildProcess {
    /// Ends it at once, without waiting.
    pub(crate) fn kill(&self) {
        self.kill.notify_one();
    }

    /// Waits up to `grace` for it to exit, as it does once its stdin has closed, and ends it if it hasn't.
    pub(crate) async fn end(&self, grace: Duration) {
        let mut exited = self.exited.clone();
        if tokio::time::timeout(grace, exited.wait_for(|done| *done)).await.is_err() {
            warn!(program = self.program, "it didn't stop in time, so it's being killed");
            self.kill();
            let _ = exited.wait_for(|done| *done).await;
        }
    }

    /// Resolves once it has exited, however that came about.
    pub(crate) fn exited(&self) -> impl Future<Output = ()> + Send + 'static {
        let mut exited = self.exited.clone();
        async move {
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
        spawn("sh", &["-c".into(), script.into()], &[], Stderr::Log(&dir.join("logs/worker.log"))).unwrap()
    }

    #[tokio::test]
    async fn ending_waits_for_a_command_that_exits_once_its_stdin_closes() {
        let dir = tempfile::tempdir().unwrap();
        let Spawned { process: command, reader, writer } = sh(dir.path(), "cat; echo stopped >&2");
        drop(writer);
        let started = Instant::now();

        command.end(Duration::from_secs(10)).await;

        assert!(started.elapsed() < Duration::from_secs(5), "it should have exited by itself");
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
