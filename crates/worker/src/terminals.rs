//! The shells a workspace's terminals run, each in a PTY in the worktree. A thread of each terminal's own keeps what it
//! shows, from everything its program writes, and sends the hub what happens in it as it happens, as "Terminals" in
//! `docs/architecture.md` says.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::TerminalId;
use protocol::terminal::TerminalSize;
use protocol::worker::{
    RunningTerminal, RunningTerminalsReply, TerminalInput, TerminalResize, TerminalShown, TerminalStart,
};
use tokio::sync::{mpsc, oneshot, watch};

use crate::serve::Outbox;

mod shell;

/// Every terminal the worker runs.
pub(crate) struct Terminals {
    inner: Arc<Mutex<Inner>>,
    /// Held while a shell starts, which opens a PTY and forks, so that happens outside `inner`'s lock, and while the
    /// worker decides what to stop, so no start adds a terminal after that. So one shell starts at a time, which also
    /// keeps two starts of one terminal from both starting it.
    starting: tokio::sync::Mutex<()>,
    outbox: Arc<Outbox>,
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
struct Running {
    number: u64,
    opened_at_ms: i64,
    /// What's typed, to the thread that writes it, in the order it came.
    typed: std::sync::mpsc::Sender<Vec<u8>>,
    /// To the thread that keeps what it shows.
    to_shell: mpsc::UnboundedSender<ToShell>,
    /// Becomes true once its shell has ended and the hub has been sent that.
    ended: watch::Receiver<bool>,
}

/// What the thread that keeps a terminal's screen is told to do, in order with what its program writes.
enum ToShell {
    Resize(TerminalSize),
    Show(oneshot::Sender<Result<TerminalShown, ApiError>>),
    Stop,
}

/// Lets go of a terminal however its thread ends, so nothing lists it or waits on it after.
struct Ending {
    inner: Arc<Mutex<Inner>>,
    terminal: TerminalId,
    number: u64,
    ended: watch::Sender<bool>,
}

impl Ending {
    /// So the worker no longer lists it, before the hub hears it ended.
    fn forget(&self) {
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
    pub(crate) fn new(outbox: Arc<Outbox>) -> Self {
        Self { inner: Arc::new(Mutex::new(Inner::default())), starting: tokio::sync::Mutex::new(()), outbox }
    }

    /// Starts a shell in the worktree, unless the terminal runs already.
    pub(crate) async fn start(&self, start: TerminalStart) -> Result<(), ApiError> {
        allowed(start.size)?;
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
        let (inner, outbox) = (self.inner.clone(), self.outbox.clone());
        // Opening a PTY and forking block, so not on one of the runtime's threads. Serving it is there too, since a
        // terminal's thread that can't start drops its runtime where it was started from, which mustn't be one of them.
        let started = tokio::task::spawn_blocking(move || {
            let (running, serving) = shell::start(start, ending, outbox)?;
            inner.lock().running.insert(terminal, running);
            // Only once it's listed, so its end, however soon it comes, lets go of it.
            serving.serve()
        });
        started.await.map_err(|e| ApiError::new(ErrorCode::Internal, format!("starting a shell failed: {e}")))?
    }

    /// Writes what was typed, for the terminal's program to read, after everything typed before it.
    pub(crate) fn input(&self, typed: TerminalInput) -> Result<(), ApiError> {
        let TerminalInput { terminal, input } = typed;
        let inner = self.inner.lock();
        let running = inner.running.get(&terminal).ok_or_else(|| not_running(&terminal))?;
        // Its writer has gone only once its program has, and the hub hears that next.
        let _ = running.typed.send(input.into_vec());
        Ok(())
    }

    pub(crate) fn resize(&self, resize: TerminalResize) -> Result<(), ApiError> {
        let TerminalResize { terminal, size } = resize;
        allowed(size)?;
        self.tell(&terminal, ToShell::Resize(size))
    }

    /// What the terminal shows now, and which of its updates come after that.
    pub(crate) async fn show(&self, terminal: &TerminalId) -> Result<TerminalShown, ApiError> {
        let (reply, shown) = oneshot::channel();
        self.tell(terminal, ToShell::Show(reply))?;
        shown.await.map_err(|_| not_running(terminal))?
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
        RunningTerminalsReply { terminals: listed.collect() }
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
