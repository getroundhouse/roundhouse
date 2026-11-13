//! One terminal's shell, in a PTY, and the threads that serve it: one reads what its program writes, one writes what's
//! typed, one waits for the shell to end, and one keeps what the terminal shows and sends the hub what happens.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::TerminalId;
use protocol::terminal::{TerminalBytes, TerminalExit, TerminalScreen, TerminalSize, TerminalUpdate, UpdateIndex};
use protocol::worker::{TerminalShown, TerminalStart, WorkerFrame};
use terminal_state::{Mirror, Size};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tracing::warn;

use super::{Ending, Running, ToShell};
use crate::serve::Outbox;

/// The most a read takes from the PTY at once.
const READ_BYTES: usize = 64 * 1024;
/// Reads waiting to be taken in. Few, so a program that writes faster than the hub reads waits in the PTY, as it would
/// for a slow terminal, rather than here.
const QUEUED_READS: usize = 4;
/// How long a shell asked to end, and the job it runs, have to end by themselves before they're killed. Long enough
/// for a program that saves as it's hung up on, such as an editor writing its recovery file, and short enough that
/// closing a tab, or archiving a workspace, doesn't seem to hang.
const STOP_GRACE: Duration = Duration::from_secs(2);
/// How often to ask whether a job that has to end by a deadline has. Often enough that a stop takes barely longer than
/// the job does to end, and asking is one system call.
#[cfg(unix)]
const STILL_RUNNING_EVERY: Duration = Duration::from_millis(20);
/// How long after the shell ends to wait for the last of what it wrote, which a program it left running can hold the
/// PTY open past. What the shell wrote as it ended is in the PTY already, so this is only for such a program, which
/// would otherwise keep the terminal open as long as it ran.
const LAST_OUTPUT: Duration = Duration::from_secs(1);
/// What the shell's programs are told the terminal is. Every system knows it, unlike Ghostty's own name, which needs a
/// description of its own installed, and Ghostty's terminal core does what it describes.
const TERM: &str = "xterm-256color";
/// Tells programs the terminal takes any colour, as Ghostty's core does, which `TERM` can't say, so a program that
/// checks, such as an editor's theme, uses its own colours rather than the nearest of 256.
const COLORTERM: &str = "truecolor";
/// What a terminal app tells the programs it runs about itself, which a hub started from one passes on, and which
/// would tell a program here that it runs in that app, as `TMUX` makes tmux refuse to start in itself. So do names
/// that start with one of [`OTHER_TERMINALS_PREFIXES`].
const OTHER_TERMINALS_ENV: &[&str] = &[
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
    "TERMINFO",
    "TERMINAL_EMULATOR",
    "COLORFGBG",
    "LC_TERMINAL",
    "LC_TERMINAL_VERSION",
    "TMUX",
    "TMUX_PANE",
    "STY",
    "WINDOW",
    "WINDOWID",
    "VTE_VERSION",
    "KONSOLE_VERSION",
    "WT_SESSION",
    "WT_PROFILE_ID",
];
const OTHER_TERMINALS_PREFIXES: &[&str] = &["GHOSTTY_", "KITTY_", "ITERM_", "WEZTERM_", "ALACRITTY_", "KONSOLE_DBUS_"];

/// A shell that has started, with what serves it, until the worker lists it.
pub(super) struct Serving {
    shell: Shell,
    runtime: tokio::runtime::Runtime,
    read: mpsc::Receiver<Vec<u8>>,
    told: mpsc::UnboundedReceiver<ToShell>,
    exit: oneshot::Receiver<TerminalExit>,
    answers: std::sync::mpsc::Sender<Vec<u8>>,
    ending: Ending,
}

impl Serving {
    /// Starts the thread that keeps what the terminal shows, or ends the shell if it can't.
    pub(super) fn serve(self) -> Result<(), ApiError> {
        let Self { shell, runtime, read, told, exit, answers, ending } = self;
        let pid = shell.pid;
        let served = spawn("terminal", move || runtime.block_on(shell.serve(read, told, exit, answers, ending)));
        served.map_err(|e| {
            if let Some(pid) = pid {
                process::signal_group(pid, process::Signal::Kill);
            }
            failed("couldn't serve the terminal", &e)
        })
    }
}

/// Starts the shell in `start.cwd`, and the threads that read and write it, and waits on it. It blocks.
pub(super) fn start(start: TerminalStart, ending: Ending, outbox: Arc<Outbox>) -> Result<(Running, Serving), ApiError> {
    let TerminalStart { terminal, size, cwd, env, opened_at_ms } = start;
    // Otherwise a worktree that has gone starts the shell in the home folder, which portable-pty falls back to.
    if !Path::new(&cwd).is_dir() {
        return Err(ApiError::new(ErrorCode::NotFound, format!("there's no folder at {cwd}")));
    }
    // The terminal's thread runs one of its own, since Ghostty's terminal can't move between threads, as a task on the
    // worker's runtime does, and a runtime gives it the timers and channels the rest of the worker has. Made here, so
    // failing to make one fails the start.
    let runtime = tokio::runtime::Builder::new_current_thread().enable_time().build();
    let runtime = runtime.map_err(|e| failed("couldn't keep what a terminal shows", &e))?;
    let pty = native_pty_system().openpty(pty_size(size)).map_err(|e| failed("couldn't open a terminal", &e))?;
    let reader = pty.master.try_clone_reader().map_err(|e| failed("couldn't read the terminal", &e))?;
    let writer = pty.master.take_writer().map_err(|e| failed("couldn't write to the terminal", &e))?;
    let (output, read) = mpsc::channel(QUEUED_READS);
    let (typed, to_type) = std::sync::mpsc::channel();
    // Before the shell, so a failure leaves nothing running. Each ends once the PTY or its channel has gone.
    spawn("terminal-reader", move || read_output(reader, &output)).map_err(|e| failed("couldn't read", &e))?;
    spawn("terminal-writer", move || write_typed(writer, &to_type)).map_err(|e| failed("couldn't write", &e))?;

    // The user's own shell, as a login shell, as a terminal app starts it, so it reads their profile. On Windows it's
    // `cmd.exe`, or whatever `ComSpec` names.
    let mut command = CommandBuilder::new_default_prog();
    command.cwd(&cwd);
    for (name, value) in env {
        command.env(name, value);
    }
    // As an agent doesn't get them, since a hub started from inside a Claude session has them, and the CLI run here
    // would take itself for part of that session.
    for name in agents::claude::SCRUBBED_ENV {
        command.env_remove(name);
    }
    let others: Vec<String> = command.iter_full_env_as_str().map(|(name, _)| name.to_string()).collect();
    for name in others.into_iter().filter(|name| another_terminals(name)) {
        command.env_remove(name);
    }
    command.env("TERM", TERM);
    command.env("COLORTERM", COLORTERM);
    let child = pty.slave.spawn_command(command).map_err(|e| failed("couldn't start a shell", &e))?;
    // Otherwise the PTY stays open once every program in it has closed it, and the reader never hears the end.
    drop(pty.slave);
    let pid = child.process_id();
    let (exited, exit) = oneshot::channel();
    if let Err(e) = spawn("terminal-waiter", move || wait(child, exited)) {
        if let Some(pid) = pid {
            process::signal_group(pid, process::Signal::Kill);
        }
        return Err(failed("couldn't wait for the shell", &e));
    }
    let next = UpdateIndex::new(0);
    let shell = Shell { terminal, size, master: pty.master, pid, front: None, outbox, next, exit: None };
    let (to_shell, told) = mpsc::unbounded_channel();
    let running = Running {
        number: ending.number,
        opened_at_ms,
        typed: typed.clone(),
        to_shell,
        ended: ending.ended.subscribe(),
    };
    Ok((running, Serving { shell, runtime, read, told, exit, answers: typed, ending }))
}

fn another_terminals(name: &str) -> bool {
    OTHER_TERMINALS_ENV.contains(&name) || OTHER_TERMINALS_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

fn failed(what: &str, e: &dyn std::fmt::Display) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("{what}: {e}"))
}

fn spawn(name: &str, run: impl FnOnce() + Send + 'static) -> std::io::Result<()> {
    std::thread::Builder::new().name(name.to_string()).spawn(run).map(drop)
}

fn read_output(mut reader: Box<dyn Read + Send>, output: &mpsc::Sender<Vec<u8>>) {
    let mut buffer = vec![0; READ_BYTES];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => {
                if output.blocking_send(buffer[..read].to_vec()).is_err() {
                    return;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            // Linux says so once every program in the terminal has closed it.
            Err(_) => return,
        }
    }
}

/// Until the terminal has gone, and with it every way to type.
fn write_typed(mut writer: Box<dyn Write + Send>, to_type: &std::sync::mpsc::Receiver<Vec<u8>>) {
    for typed in to_type {
        if writer.write_all(&typed).and_then(|()| writer.flush()).is_err() {
            return;
        }
    }
}

fn wait(mut child: Box<dyn Child + Send + Sync>, exited: oneshot::Sender<TerminalExit>) {
    let status = child.wait();
    let code = status.ok().filter(|status| status.signal().is_none()).map(|status| status.exit_code().cast_signed());
    let _ = exited.send(TerminalExit { code });
}

/// What the thread that keeps a terminal's screen holds.
struct Shell {
    terminal: TerminalId,
    size: TerminalSize,
    master: Box<dyn MasterPty + Send>,
    /// The shell's, which leads a process group of its own.
    pid: Option<u32>,
    /// The group of the job the shell last ran in front, as it was signalled, which an interactive shell starts in a
    /// group of its own, and which can outlast the shell.
    front: Option<u32>,
    outbox: Arc<Outbox>,
    /// The `at` the next update gets, which is how many it has sent.
    next: UpdateIndex,
    /// How the shell ended, once it has.
    exit: Option<TerminalExit>,
}

impl Shell {
    /// Until the shell has ended and the last of what it wrote is in, then lets the hub know it ended.
    async fn serve(
        mut self,
        mut read: mpsc::Receiver<Vec<u8>>,
        mut told: mpsc::UnboundedReceiver<ToShell>,
        mut exit: oneshot::Receiver<TerminalExit>,
        answers: std::sync::mpsc::Sender<Vec<u8>>,
        ending: Ending,
    ) {
        // Made here, since it can't be sent between threads. It answers what the program asks of its terminal.
        let mirror = Mirror::answering(mirror_size(self.size), move |answer| {
            let _ = answers.send(answer.to_vec());
        });
        match mirror {
            Ok(mut mirror) => self.until_ended(&mut mirror, &mut read, &mut told, &mut exit).await,
            Err(e) => {
                warn!("couldn't keep what terminal {} shows, so its shell is ended: {e}", self.terminal);
                self.signal(process::Signal::Kill);
            }
        }
        ending.forget();
        let exited = self.exit.unwrap_or(TerminalExit { code: None });
        self.send(TerminalUpdate::Exited(exited)).await;
    }

    async fn until_ended(
        &mut self,
        mirror: &mut Mirror,
        read: &mut mpsc::Receiver<Vec<u8>>,
        told: &mut mpsc::UnboundedReceiver<ToShell>,
        exit: &mut oneshot::Receiver<TerminalExit>,
    ) {
        let (mut kill_at, mut done_at, mut read_all, mut untold) = (None, None, false, false);
        while !(read_all && self.exit.is_some()) {
            // In this order, so neither a stop nor the end waits out a program that writes without a pause.
            tokio::select! {
                biased;
                told = told.recv(), if !untold => match told {
                    Some(ToShell::Resize(size)) => self.resize(mirror, size).await,
                    Some(ToShell::Show(reply)) => {
                        let _ = reply.send(self.shown(mirror));
                    }
                    Some(ToShell::Stop) => self.hang_up(&mut kill_at),
                    // Nothing can tell it anything once the worker has let go of it.
                    None => {
                        untold = true;
                        self.hang_up(&mut kill_at);
                    }
                },
                ended = &mut *exit, if self.exit.is_none() => {
                    self.exit = Some(ended.unwrap_or(TerminalExit { code: None }));
                    done_at = Some(Instant::now() + LAST_OUTPUT);
                }
                () = until(done_at) => break,
                () = until(kill_at) => {
                    self.signal(process::Signal::Kill);
                    kill_at = None;
                }
                output = read.recv(), if !read_all => match output {
                    Some(output) => self.wrote(mirror, output).await,
                    None => read_all = true,
                },
            }
        }
        #[cfg(unix)]
        if let Some(kill_at) = kill_at {
            self.outlasted_by_front(kill_at).await;
        }
    }

    /// Waits for the job the shell ran in front, which can outlast the shell, and the PTY, which a Mac closes as the
    /// shell ends, and kills it if it hasn't ended by `kill_at`.
    #[cfg(unix)]
    async fn outlasted_by_front(&self, kill_at: Instant) {
        let Some(front) = self.front.filter(|front| Some(*front) != self.pid) else { return };
        while process::group_running(front) {
            if Instant::now() >= kill_at {
                process::signal_group(front, process::Signal::Kill);
                return;
            }
            tokio::time::sleep(STILL_RUNNING_EVERY).await;
        }
    }

    /// Asks the shell to end, and the job it runs, and kills them if they haven't by `kill_at`.
    fn hang_up(&mut self, kill_at: &mut Option<Instant>) {
        if kill_at.is_none() && self.exit.is_none() {
            self.signal(process::Signal::Hangup);
            *kill_at = Some(Instant::now() + STOP_GRACE);
        }
    }

    async fn wrote(&mut self, mirror: &mut Mirror, output: Vec<u8>) {
        mirror.write(&output);
        self.send(TerminalUpdate::Output { output: TerminalBytes::new(output) }).await;
    }

    /// Gives the worker's copy the new size, and the PTY, which tells the program, and says so in order with the
    /// output. The same size again changes nothing, and so does one either can't take, so the two never differ.
    async fn resize(&mut self, mirror: &mut Mirror, size: TerminalSize) {
        if size == self.size {
            return;
        }
        if let Err(e) = mirror.resize(mirror_size(size)) {
            warn!("couldn't resize the copy of terminal {}: {e}", self.terminal);
            return;
        }
        if let Err(e) = self.master.resize(pty_size(size)) {
            warn!("couldn't resize terminal {}: {e}", self.terminal);
            if let Err(e) = mirror.resize(mirror_size(self.size)) {
                warn!("couldn't put the copy of terminal {} back to its size: {e}", self.terminal);
            }
            return;
        }
        self.size = size;
        self.send(TerminalUpdate::Resized { size }).await;
    }

    fn shown(&self, mirror: &Mirror) -> Result<TerminalShown, ApiError> {
        let snapshot = mirror.snapshot().map_err(|e| {
            ApiError::new(ErrorCode::Worker, format!("couldn't say what terminal {} shows: {e}", self.terminal))
        })?;
        let screen = TerminalScreen { size: self.size, bytes: TerminalBytes::new(snapshot) };
        Ok(TerminalShown { screen, next: self.next })
    }

    async fn send(&mut self, update: TerminalUpdate) {
        let at = self.next;
        self.next = at.next();
        self.outbox.send(WorkerFrame::Terminal { terminal: self.terminal.clone(), at, update }).await;
    }

    /// Signals the shell's group, only before the shell has ended, since its PID can be another process's after, and
    /// the group of the job it runs in front. A shell that's hung up on sends it on to the jobs it runs behind, so
    /// those end too, but for one started to outlive the terminal, such as with `nohup`.
    fn signal(&mut self, signal: process::Signal) {
        // Before the shell is signalled, since the PTY says which it is only while the shell runs.
        #[cfg(unix)]
        if let Some(front) = self.master.process_group_leader().and_then(|group| u32::try_from(group).ok()) {
            self.front = Some(front);
        }
        if let Some(pid) = self.pid.filter(|_| self.exit.is_none()) {
            process::signal_group(pid, signal);
        }
        if let Some(front) = self.front.filter(|front| Some(*front) != self.pid) {
            process::signal_group(front, signal);
        }
    }
}

/// Never, without a deadline.
async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

fn pty_size(size: TerminalSize) -> PtySize {
    PtySize { rows: size.rows, cols: size.cols, pixel_width: 0, pixel_height: 0 }
}

fn mirror_size(size: TerminalSize) -> Size {
    Size { cols: size.cols, rows: size.rows }
}
