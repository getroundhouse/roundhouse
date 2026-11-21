//! A run of Codex or Antigravity, from its launch to its exit: the process group it's in, whether it's gone and whether
//! its stop was asked for, and where what it says goes, which closes once it has said it exited.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use protocol::event::EventBody;
use tokio::sync::{mpsc, watch};

use crate::AgentOutput;

/// How long a CLI has to exit once it's asked to, which is time to stop its own tools, before its group is killed.
const STOP_GRACE: Duration = Duration::from_secs(3);

pub(crate) struct Run {
    /// The process group's, which is its CLI's. Antigravity replaces its CLI during a run, and with it this.
    pid: AtomicU32,
    /// Set as soon as the CLI is known to be gone, which is before what it said has all been read.
    gone: AtomicBool,
    requested: AtomicBool,
    /// Set once the run has said it exited.
    exited: watch::Sender<bool>,
    pid_file: Option<PathBuf>,
    output: tokio::sync::Mutex<Option<mpsc::Sender<AgentOutput>>>,
}

impl Run {
    /// A run of the CLI started as `pid`, recorded at `pid_file`, which says what it does to `output`.
    pub(crate) fn new(pid: u32, pid_file: Option<PathBuf>, output: mpsc::Sender<AgentOutput>) -> Run {
        Run {
            pid: AtomicU32::new(pid),
            gone: AtomicBool::new(false),
            requested: AtomicBool::new(false),
            exited: watch::channel(false).0,
            pid_file,
            output: tokio::sync::Mutex::new(Some(output)),
        }
    }

    pub(crate) fn pid(&self) -> u32 {
        self.pid.load(Ordering::SeqCst)
    }

    /// Makes `pid` the CLI the run is in, once the one before it has exited.
    pub(crate) fn replaced(&self, pid: u32) {
        self.pid.store(pid, Ordering::SeqCst);
    }

    pub(crate) fn is_gone(&self) -> bool {
        self.gone.load(Ordering::SeqCst)
    }

    pub(crate) fn mark_gone(&self) {
        self.gone.store(true, Ordering::SeqCst);
    }

    /// Whether the run was asked to stop, so its exit was too.
    pub(crate) fn requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }

    pub(crate) async fn emit(&self, output: AgentOutput) {
        let mut sender = self.output.lock().await;
        let closes = matches!(output, AgentOutput::Exited { .. });
        if let Some(sender) = sender.as_ref() {
            let _ = sender.send(output).await;
        }
        if closes {
            *sender = None;
        }
    }

    pub(crate) async fn event(&self, body: EventBody, raw: Option<String>) {
        self.emit(AgentOutput::event(body, raw)).await;
    }

    /// Asks the CLI to exit, and kills its group if it hasn't in [`STOP_GRACE`]. Returns once the run has said it
    /// exited.
    pub(crate) async fn stop(&self) {
        self.requested.store(true, Ordering::SeqCst);
        stop_group(|| self.pid(), self.exited.subscribe()).await;
    }

    /// Kills the group unless the CLI is gone already, as dropping the run's handle does, without waiting. The
    /// supervisor owns the child, but only the handle owns its lifetime, so cancelling a start or a one-off call kills
    /// the group even before the CLI has answered.
    pub(crate) fn kill_unless_gone(&self) {
        if !self.is_gone() {
            self.requested.store(true, Ordering::SeqCst);
            process::signal_group(self.pid(), process::Signal::Kill);
        }
    }

    /// Ends the run: forgets its record, says it exited, as `code` or `signal`, with what it last wrote to stderr, and
    /// tells whatever waits on its exit.
    pub(crate) async fn exit(&self, code: Option<i32>, signal: Option<i32>, stderr_tail: Vec<String>, requested: bool) {
        if let Some(path) = &self.pid_file {
            let _ = tokio::fs::remove_file(path).await;
        }
        self.emit(AgentOutput::Exited { code, signal, stderr_tail, requested }).await;
        self.exited.send_replace(true);
    }
}

/// Asks the group of the CLI `pid` says to exit, and kills it if `exited` hasn't said so in [`STOP_GRACE`], then waits
/// for it. `pid` is read again before the kill, since the CLI may have been replaced meanwhile. Once `exited` has said
/// so, the CLI has been reaped and its pid may be another process's, so it's left alone.
pub(crate) async fn stop_group(pid: impl Fn() -> u32, mut exited: watch::Receiver<bool>) {
    if *exited.borrow() {
        return;
    }
    process::signal_group(pid(), process::Signal::Terminate);
    if tokio::time::timeout(STOP_GRACE, exited.wait_for(|done| *done)).await.is_err() {
        process::signal_group(pid(), process::Signal::Kill);
        let _ = exited.wait_for(|done| *done).await;
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::AgentRunState;

    use super::*;

    #[tokio::test]
    async fn a_run_that_exits_says_so_once_forgets_its_record_and_signals_nothing_after() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("run.pid");
        std::fs::write(&record, "1").unwrap();
        let (output, mut said) = mpsc::channel(8);
        let run = Run::new(1, Some(record.clone()), output);

        run.exit(Some(0), None, vec!["bye".into()], true).await;
        run.event(EventBody::AgentState { state: AgentRunState::Idle }, None).await;
        stop_group(|| panic!("a run that has exited was signalled"), run.exited.subscribe()).await;

        let exited = said.recv().await;
        assert!(
            matches!(&exited, Some(AgentOutput::Exited { code: Some(0), signal: None, stderr_tail, requested: true })
                if *stderr_tail == ["bye"]),
            "{exited:?}"
        );
        assert!(said.recv().await.is_none(), "the run said more after it exited");
        assert!(!record.exists());
    }
}
