//! The terminals a worker runs, by their IDs, and what it holds of each while its shell runs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::TerminalId;
use protocol::paths;
use protocol::terminal::{
    MOST_KEPT_POINTS, TerminalBoundsReply, TerminalFitReply, TerminalRowsReply, TerminalSize, TerminalTextReply, Typed,
};
use protocol::worker::{
    RunningTerminal, RunningTerminalsReply, TerminalBounds, TerminalFit, TerminalRows, TerminalStart, TerminalText,
    TerminalTyped, TerminalWatch,
};
use tokio::sync::{mpsc, oneshot, watch};

use super::shell::{self, Asked};
use crate::Timers;
use crate::serve::{HubConnection, Outbox};

/// Every terminal the worker runs.
pub(crate) struct Terminals {
    inner: Arc<Mutex<Inner>>,
    /// Held while a shell starts, which opens a PTY and forks, so that happens outside `inner`'s lock, and while the
    /// worker decides what to stop, so no start adds a terminal after that. So one shell starts at a time, which also
    /// keeps two starts of one terminal from both starting it.
    starting: tokio::sync::Mutex<()>,
    outbox: Arc<Outbox>,
    /// Where each keeps the process records of its shell and the job it runs in front, which is where the worker
    /// records its agents.
    process_records_dir: PathBuf,
    timers: Timers,
}

#[derive(Default)]
struct Inner {
    running: HashMap<TerminalId, Running>,
    /// How many have started, which numbers each, so one that ends can't let go of another started with its ID.
    started: u64,
    /// Set once the worker is stopping, so a start can't add a terminal after it has decided what to stop.
    closed: bool,
}

/// What the worker holds of a terminal whose shell hasn't ended.
pub(super) struct Running {
    pub(super) number: u64,
    pub(super) opened_at_ms: i64,
    /// To the thread that keeps what it shows.
    pub(super) to_shell: mpsc::UnboundedSender<ToShell>,
    /// Becomes true once its shell has ended and the hub has been sent that.
    pub(super) ended: watch::Receiver<bool>,
}

/// What the thread that keeps a terminal's screen is told to do, in order with what its program writes.
pub(super) enum ToShell {
    /// A new size, keeping the cells the hub names.
    Fit(TerminalFit, oneshot::Sender<Result<TerminalFitReply, ApiError>>),
    Typed(Typed),
    /// What the hub asks of what it shows.
    Asked(Asked),
    /// Watched by the hub on that connection from now, or by none.
    Watch(Option<HubConnection>),
    Stop,
}

/// Lets go of a terminal however its thread ends, so nothing lists it or waits on it after.
pub(super) struct Ending {
    inner: Arc<Mutex<Inner>>,
    terminal: TerminalId,
    pub(super) number: u64,
    pub(super) ended: watch::Sender<bool>,
}

impl Ending {
    /// So the worker no longer lists it, before the hub hears it ended.
    pub(super) fn forget(&self) {
        let mut inner = self.inner.lock();
        if inner.running.get(&self.terminal).is_some_and(|running| running.number == self.number) {
            inner.running.remove(&self.terminal);
        }
    }
}

impl Drop for Ending {
    fn drop(&mut self) {
        self.forget();
        self.ended.send_replace(true);
    }
}

impl Terminals {
    pub(crate) fn new(outbox: Arc<Outbox>, process_records_dir: PathBuf, timers: Timers) -> Self {
        let inner = Arc::new(Mutex::new(Inner::default()));
        Self { inner, starting: tokio::sync::Mutex::new(()), outbox, process_records_dir, timers }
    }

    /// Starts a shell in the worktree, unless the terminal runs already.
    pub(crate) async fn start(&self, start: TerminalStart, given: Arc<Environment>) -> Result<(), ApiError> {
        allowed(start.size)?;
        // Its ID names the files its process records are in. A hub refuses one that can't, but an older one doesn't.
        paths::file_name_id("a terminal's", start.terminal.as_str())?;
        let _starting = self.starting.lock().await;
        let number = {
            let mut inner = self.inner.lock();
            if inner.closed {
                return Err(ApiError::new(ErrorCode::Worker, "the worker is stopping"));
            }
            if inner.running.contains_key(&start.terminal) {
                return Ok(());
            }
            inner.started += 1;
            inner.started
        };
        let terminal = start.terminal.clone();
        let ended = watch::Sender::new(false);
        let ending = Ending { inner: self.inner.clone(), terminal: terminal.clone(), number, ended };
        let (inner, outbox, timers) = (self.inner.clone(), self.outbox.clone(), self.timers);
        let records_dir = self.process_records_dir.clone();
        // Opening a PTY and forking block, so not on one of the runtime's threads. Serving it is there too, since a
        // terminal's thread that can't start drops its runtime where it was started from, which mustn't be one of them.
        let started = tokio::task::spawn_blocking(move || {
            let (running, serving) = shell::start(start, &records_dir, timers, &given, ending, outbox)?;
            inner.lock().running.insert(terminal, running);
            // Only once it's listed, so its end, however soon it comes, lets go of it.
            serving.serve()
        });
        started.await.map_err(|e| ApiError::new(ErrorCode::Internal, format!("starting a shell failed: {e}")))?
    }

    /// Writes what the program reads for what was pressed, after the sizes and everything pressed before it.
    pub(crate) fn typed(&self, typed: TerminalTyped) -> Result<(), ApiError> {
        let TerminalTyped { terminal, typed } = typed;
        self.tell(&terminal, ToShell::Typed(typed))
    }

    /// Gives the terminal a new size, after the sizes and everything pressed before it, and answers with where the
    /// cells the hub named went, once it has.
    pub(crate) fn fit(
        &self,
        fit: TerminalFit,
    ) -> Result<impl Future<Output = Result<TerminalFitReply, ApiError>> + use<>, ApiError> {
        allowed(fit.size)?;
        if fit.keep.len() > MOST_KEPT_POINTS {
            let why = format!("a fit keeps at most {MOST_KEPT_POINTS} cells, not {}", fit.keep.len());
            return Err(ApiError::new(ErrorCode::InvalidRequest, why));
        }
        let terminal = fit.terminal.clone();
        let (reply, fitted) = oneshot::channel();
        self.tell(&terminal, ToShell::Fit(fit, reply))?;
        Ok(async move { fitted.await.map_err(|_| not_running(&terminal))? })
    }

    /// Whether the hub on `hub` watches the terminal, as [`TerminalWatch`] says.
    pub(crate) fn watch(&self, watch: TerminalWatch, hub: Option<HubConnection>) -> Result<(), ApiError> {
        let TerminalWatch { terminal, watching } = watch;
        self.tell(&terminal, ToShell::Watch(hub.filter(|_| watching)))
    }

    /// Rows of the terminal's history by number, as [`TerminalRows`] says.
    pub(crate) async fn rows(&self, asked: TerminalRows) -> Result<TerminalRowsReply, ApiError> {
        let terminal = asked.terminal.clone();
        self.ask(&terminal, |reply| Asked::Rows(asked, reply)).await
    }

    /// The ends of a word or a line, as [`TerminalBounds`] says.
    pub(crate) async fn bounds(&self, asked: TerminalBounds) -> Result<TerminalBoundsReply, ApiError> {
        let terminal = asked.terminal.clone();
        self.ask(&terminal, |reply| Asked::Bounds(asked, reply)).await
    }

    /// The text of cells, a page at a time, as [`TerminalText`] says.
    pub(crate) async fn text(&self, asked: TerminalText) -> Result<TerminalTextReply, ApiError> {
        let terminal = asked.terminal.clone();
        self.ask(&terminal, |reply| Asked::Text(asked, reply)).await
    }

    async fn ask<T>(
        &self,
        terminal: &TerminalId,
        asked: impl FnOnce(oneshot::Sender<Result<T, ApiError>>) -> Asked,
    ) -> Result<T, ApiError> {
        let (reply, answer) = oneshot::channel();
        self.tell(terminal, ToShell::Asked(asked(reply)))?;
        answer.await.map_err(|_| not_running(terminal))?
    }

    /// Ends the terminal's shell, and the job it was running, and waits until the hub has been sent that it ended.
    pub(crate) async fn stop(&self, terminal: &TerminalId) {
        let ended = {
            let inner = self.inner.lock();
            let Some(running) = inner.running.get(terminal) else { return };
            let _ = running.to_shell.send(ToShell::Stop);
            running.ended.clone()
        };
        wait_for(ended).await;
    }

    /// Ends every terminal, as the worker stops, and lets no more start.
    pub(crate) async fn stop_all(&self) {
        let mut stopping = Vec::new();
        {
            // Once a shell starting has, so it's stopped with the rest.
            let _starting = self.starting.lock().await;
            let mut inner = self.inner.lock();
            inner.closed = true;
            for running in inner.running.values() {
                let _ = running.to_shell.send(ToShell::Stop);
                stopping.push(running.ended.clone());
            }
        }
        // All at once, so this takes as long as the slowest.
        for ended in stopping {
            wait_for(ended).await;
        }
    }

    pub(crate) fn running(&self) -> RunningTerminalsReply {
        let inner = self.inner.lock();
        let listed = inner.running.iter().map(|(terminal, running)| RunningTerminal {
            terminal: terminal.clone(),
            opened_at_ms: running.opened_at_ms,
        });
        RunningTerminalsReply { terminals: listed.collect(), rows: true }
    }

    fn tell(&self, terminal: &TerminalId, told: ToShell) -> Result<(), ApiError> {
        let inner = self.inner.lock();
        let running = inner.running.get(terminal).ok_or_else(|| not_running(terminal))?;
        running.to_shell.send(told).map_err(|_| not_running(terminal))
    }
}

fn allowed(size: TerminalSize) -> Result<(), ApiError> {
    if size.is_allowed() {
        return Ok(());
    }
    let TerminalSize { cols, rows } = size;
    let TerminalSize { cols: most_cols, rows: most_rows } = TerminalSize::MOST;
    let why = format!("a terminal has from 1 to {most_cols} columns and 1 to {most_rows} rows, not {cols} by {rows}");
    Err(ApiError::new(ErrorCode::InvalidRequest, why))
}

/// Once the terminal's thread has let go of it, however that came about.
async fn wait_for(mut ended: watch::Receiver<bool>) {
    let _ = ended.wait_for(|ended| *ended).await;
}

fn not_running(terminal: &TerminalId) -> ApiError {
    ApiError::new(ErrorCode::NotFound, format!("terminal {terminal} isn't running"))
}
