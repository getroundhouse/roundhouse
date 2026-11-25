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

use crate::machine_environment::MachineEnvironment;

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

/// Starts `program`, given `machine`'s variables and its command's `own` over them, over what it inherits from the
/// hub, as the leader of a process group of its own, so Ctrl-C on a foreground hub goes through the hub's clean
/// shutdown, and ending it ends whatever it started.
pub(crate) fn spawn(
    program: &str,
    args: &[String],
    own: &[(String, String)],
    machine: &MachineEnvironment,
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
    child.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(stderr).kill_on_drop(true);
    machine.with(own).apply(child.as_std_mut());
    process::own_group(child.as_std_mut());
    let mut child = child.spawn().map_err(context(format!("starting {program}")))?;
    let (Some(writer), Some(reader)) = (child.stdin.take(), child.stdout.take()) else {
        return Err(io::Error::other(format!("{program} started without its pipes")));
    };
    info!(name: "child.started", pid = child.id(), program, "Started a child of the hub");
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
            warn!(
                name: "child.not_stopped",
                program = self.program,
                "A child of the hub didn't stop in time, so it's being killed"
            );
            self.kill();
            let _ = exited.wait_for(|done| *done).await;
        }
    }

    /// One that has exited already, for a stream with no child behind it.
    #[cfg(test)]
    pub(crate) fn already_exited() -> Self {
        Self { program: "nothing".to_string(), kill: Arc::new(Notify::new()), exited: watch::channel(true).1 }
    }

    /// One that exits only once it's killed, for a stand-in whose stop takes all its grace.
    #[cfg(test)]
    pub(crate) fn until_killed() -> Self {
        let kill = Arc::new(Notify::new());
        let (exits, exited) = watch::channel(false);
        let killed = kill.clone();
        tokio::spawn(async move {
            killed.notified().await;
            exits.send_replace(true);
        });
        Self { program: "a stand-in".to_string(), kill, exited }
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
    use tokio::io::{AsyncBufReadExt, BufReader};

    use super::*;

    fn sh(dir: &Path, script: &str) -> Spawned {
        let (args, machine) = (["-c".into(), script.into()], MachineEnvironment::only(&[]));
        spawn("sh", &args, &[], &machine, Stderr::Log(&dir.join("logs/worker.log"))).unwrap()
    }

    /// As a worker's command, a preflight and a provider's adapter are started.
    #[tokio::test]
    async fn a_child_has_the_machines_environment_with_its_commands_own_over_it() {
        let machine = MachineEnvironment::only(&[("FROM_THE_MACHINE", "the machine's"), ("BOTH", "the machine's")]);
        let args = ["-c".into(), r#"echo "$FROM_THE_MACHINE, $BOTH""#.into()];
        let own = [("BOTH".to_string(), "the command's".to_string())];

        let Spawned { reader, .. } = spawn("sh", &args, &own, &machine, Stderr::Hubs).unwrap();

        let mut line = String::new();
        BufReader::new(reader).read_line(&mut line).await.unwrap();
        assert_eq!(line, "the machine's, the command's\n");
    }

    #[tokio::test]
    async fn ending_waits_for_a_command_that_exits_once_its_stdin_closes() {
        let dir = tempfile::tempdir().unwrap();
        let Spawned { process: command, reader, writer } = sh(dir.path(), "cat; echo stopped >&2");
        drop(writer);

        // A grace far longer than the test waits, so ending that waits it out rather than for the command fails.
        let ended = tokio::time::timeout(Duration::from_secs(10), command.end(Duration::from_mins(10))).await;

        ended.expect("ending waited out its grace for a command that had exited");
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

        waiting::task::until(Duration::from_secs(5), "what the command started outlived it", || {
            !process::testing::is_running(helper)
        })
        .await;
    }
}
