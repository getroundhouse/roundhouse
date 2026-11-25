//! A run of an agent's CLI, from its launch to its exit: the process group it's in, whether it's gone and whether its
//! stop was asked for, and where what it says goes, which closes once it has said it exited.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use protocol::event::EventBody;
use tokio::sync::{mpsc, watch};
use tracing::{Span, warn};

use crate::AgentOutput;

/// How long a CLI has to exit once it's asked to, which is time to stop its own tools, before its group is killed.
const STOP_GRACE: Duration = Duration::from_secs(3);

pub(crate) struct Run {
    /// The process group's, which is its CLI's. It changes when Antigravity's adapter replaces its CLI, and when
    /// Claude's starts its CLI again to correct a launch. It's 0 from the CLI's reaping until another replaces it, since
    /// the reaped one's pid may be another process's by then.
    pid: AtomicU32,
    /// Set as soon as the CLI is known to be gone, which is before what it said has all been read.
    gone: AtomicBool,
    /// Set once the run is asked to stop, which a launch that's still being started waits on too.
    requested: watch::Sender<bool>,
    /// Set once the run has said it exited.
    exited: watch::Sender<bool>,
    pid_file: Option<PathBuf>,
    output: tokio::sync::Mutex<Option<mpsc::Sender<AgentOutput>>>,
    /// The span it was launched in, which says whose it is, for a task begun for it outside any of its own.
    span: Span,
}

impl Run {
    /// A run of the CLI started as `pid`, recorded at `pid_file`, which says what it does to `output`.
    pub(crate) fn new(pid: u32, pid_file: Option<PathBuf>, output: mpsc::Sender<AgentOutput>) -> Run {
        Run {
            pid: AtomicU32::new(pid),
            gone: AtomicBool::new(false),
            requested: watch::channel(false).0,
            exited: watch::channel(false).0,
            pid_file,
            output: tokio::sync::Mutex::new(Some(output)),
            span: Span::current(),
        }
    }

    /// What the log has of the run, for a task begun for it.
    pub(crate) fn span(&self) -> Span {
        self.span.clone()
    }

    pub(crate) fn pid(&self) -> u32 {
        self.pid.load(Ordering::SeqCst)
    }

    /// Makes `pid` the CLI the run is in, once the one before it has exited.
    pub(crate) fn replaced(&self, pid: u32) {
        self.pid.store(pid, Ordering::SeqCst);
    }

    /// The CLI has exited and been reaped, so nothing signals its pid from now on.
    pub(crate) fn reaped(&self) {
        self.pid.store(0, Ordering::SeqCst);
    }

    pub(crate) fn is_gone(&self) -> bool {
        self.gone.load(Ordering::SeqCst)
    }

    pub(crate) fn mark_gone(&self) {
        self.gone.store(true, Ordering::SeqCst);
    }

    /// Whether the run was asked to stop, so its exit was too.
    pub(crate) fn requested(&self) -> bool {
        *self.requested.borrow()
    }

    /// Returns once the run has been asked to stop, at once if it has been already.
    pub(crate) async fn stop_asked(&self) {
        let _ = self.requested.subscribe().wait_for(|requested| *requested).await;
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
        self.requested.send_replace(true);
        stop_group(|| self.pid(), self.exited.subscribe()).await;
    }

    /// Asks the CLI to exit without waiting, for a CLI the adapter can't go on with, so its exit says it wasn't asked
    /// for.
    pub(crate) fn terminate(&self) {
        process::signal_group(self.pid(), process::Signal::Terminate);
    }

    /// Kills the group unless the CLI is gone already, as dropping the run's handle does, without waiting. The
    /// supervisor owns the child, but only the handle owns its lifetime, so cancelling a start or a one-off call kills
    /// the group even before the CLI has answered.
    pub(crate) fn kill_unless_gone(&self) {
        if !self.is_gone() {
            self.requested.send_replace(true);
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
/// for it. `pid` is read again before the kill, since the CLI may have been replaced or reaped meanwhile. Once `exited`
/// has said so, the CLI has been reaped and its pid may be another process's, so it's left alone.
pub(crate) async fn stop_group(pid: impl Fn() -> u32, mut exited: watch::Receiver<bool>) {
    if *exited.borrow() {
        return;
    }
    process::signal_group(pid(), process::Signal::Terminate);
    if tokio::time::timeout(STOP_GRACE, exited.wait_for(|done| *done)).await.is_err() {
        let pid = pid();
        // A pid of 0 is a CLI that has been reaped, whose output is still being read, so it ignored nothing.
        if pid != 0 {
            warn!(
                name: "agent.process_not_stopped",
                pid,
                grace = ?STOP_GRACE,
                "A CLI hadn't exited after SIGTERM, so its group is killed"
            );
            process::signal_group(pid, process::Signal::Kill);
        }
        let _ = exited.wait_for(|done| *done).await;
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::AgentRunState;
    use tracing::error_span;

    use super::*;

    #[test]
    fn a_runs_span_is_the_one_it_was_launched_in_wherever_its_asked_for() {
        let logs = logging::testing::Logs::capture();
        let launched = error_span!("agent", session_id = "s");
        let run = launched.in_scope(|| Run::new(1, None, mpsc::channel(1).0));

        error_span!("another", session_id = "another").in_scope(|| {
            run.span().in_scope(|| warn!(name: "a.run", "A task of the run's"));
        });

        assert_eq!(logs.events()[0]["context"]["session_id"], "s");
    }

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

    /// On Unix, where a CLI can ignore SIGTERM. On Windows every signal ends the CLI at once.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_cli_that_ignores_sigterm_is_killed_once_the_grace_is_up() {
        let dir = tempfile::tempdir().unwrap();
        let ready = dir.path().join("ready");
        let sh = std::path::Path::new("/bin/sh");
        let mut command = crate::starting::piped(sh, dir.path());
        command.args(["-c", "trap '' TERM; : > ready; exec sleep 100"]);
        let environment = process::testing::WholeEnvironment::for_a_test(dir.path()).given();
        let mut child = crate::starting::start(&mut command, sh, dir.path(), &environment).unwrap();
        let (output, mut said) = mpsc::channel(8);
        let run = std::sync::Arc::new(Run::new(child.id().unwrap(), None, output));
        tokio::spawn({
            let run = run.clone();
            async move {
                let status = child.wait().await.unwrap();
                run.reaped();
                run.mark_gone();
                run.exit(status.code(), crate::stdio::signal_of(status), Vec::new(), run.requested()).await;
            }
        });
        waiting::task::until(Duration::from_secs(5), "the CLI ignores SIGTERM", || ready.exists()).await;
        // So the grace passes at once. Nothing else waits on the clock, which would jump past it too.
        tokio::time::pause();

        run.stop().await;

        let exited = said.recv().await;
        assert!(matches!(exited, Some(AgentOutput::Exited { signal: Some(9), requested: true, .. })), "{exited:?}");
    }
}
