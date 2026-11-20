//! One terminal's shell, in a PTY, and the threads that serve it: one reads what its program writes, one writes what's
//! typed, one waits for the shell to end, and one keeps what the terminal shows and sends the hub what happens.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use portable_pty::{Child, MasterPty, PtyPair, PtySize, PtySystem, native_pty_system};
use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::TerminalId;
use protocol::terminal::{TerminalBytes, TerminalExit, TerminalScreen, TerminalSize, TerminalUpdate};
use protocol::worker::{TerminalShown, TerminalStart};
use terminal_state::{Mirror, Size};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tracing::warn;

use super::command;
use super::front::{self, Front};
use super::sending::Sending;
use crate::Timers;
use crate::serve::Outbox;
use crate::terminals::process_records::ProcessRecords;
use crate::terminals::running::{Ending, Running, ToShell};

/// The most a read takes from the PTY at once.
const READ_BYTES: usize = 64 * 1024;
/// Reads waiting to be taken in. Few, so a program that writes faster than the hub reads waits in the PTY, as it would
/// for a slow terminal, rather than here.
const QUEUED_READS: usize = 4;
/// How long after the shell ends to wait for the last of what it wrote, which a program it left running can hold the
/// PTY open past. What the shell wrote as it ended is in the PTY already, so this is only for such a program, which
/// would otherwise keep the terminal open as long as it ran.
const LAST_OUTPUT: Duration = Duration::from_secs(1);
/// A shell that has started, with what serves it, until the worker lists it.
pub(in crate::terminals) struct Serving {
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
    pub(in crate::terminals) fn serve(self) -> Result<(), ApiError> {
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

/// Held while a PTY opens. macOS's `openpty` isn't safe to call from two threads at once: called from sixteen together,
/// about one call in three hundred fails, with errno -6, and none does one at a time. A worker starts one terminal at a
/// time, but a process can serve more than one worker, as the worker's own tests do.
pub(super) static OPENING_A_PTY: Mutex<()> = Mutex::new(());

/// Starts the shell in `start.cwd`, given what every process the worker starts is, and the threads that read and write
/// it, and waits on it, with the worker's `timers`, and keeps the process records of what it runs in `records_dir`. It
/// blocks.
pub(in crate::terminals) fn start(
    start: TerminalStart,
    records_dir: &Path,
    timers: Timers,
    given: &Environment,
    ending: Ending,
    outbox: Arc<Outbox>,
) -> Result<(Running, Serving), ApiError> {
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
    let pty = open_pty(&*native_pty_system(), size)?;
    let reader = pty.master.try_clone_reader().map_err(|e| failed("couldn't read the terminal", &e))?;
    let writer = pty.master.take_writer().map_err(|e| failed("couldn't write to the terminal", &e))?;
    let (output, read) = mpsc::channel(QUEUED_READS);
    let (typed, to_type) = std::sync::mpsc::channel();
    // Before the shell, so a failure leaves nothing running. Each ends once the PTY or its channel has gone.
    spawn("terminal-reader", move || read_output(reader, &output)).map_err(|e| failed("couldn't read", &e))?;
    spawn("terminal-writer", move || write_typed(writer, &to_type)).map_err(|e| failed("couldn't write", &e))?;
    let process_records = ProcessRecords::new(records_dir, &terminal);

    let child = pty.slave.spawn_command(command::shell(&cwd, env, given));
    let child = child.map_err(|e| failed("couldn't start a shell", &e))?;
    // Otherwise the PTY stays open once every program in it has closed it, and the reader never hears the end.
    drop(pty.slave);
    let pid = child.process_id();
    if let Some(pid) = pid {
        process_records.shell(pid);
    }
    let (exited, exit) = oneshot::channel();
    if let Err(e) = spawn("terminal-waiter", move || wait(child, exited)) {
        if let Some(pid) = pid {
            process::signal_group(pid, process::Signal::Kill);
        }
        return Err(failed("couldn't wait for the shell", &e));
    }
    let (master, stop_grace, look_every) = (pty.master, timers.terminal_stop_grace, timers.look_in_front_every);
    let (front, sending) = (Front::Looked(None), Sending::new(terminal.clone(), outbox));
    let shell =
        Shell { terminal, size, master, pid, front, process_records, look_every, stop_grace, sending, exit: None };
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

/// A PTY of `size`, from `system`, opened while no other thread is opening one, as [`OPENING_A_PTY`] says.
pub(super) fn open_pty(system: &dyn PtySystem, size: TerminalSize) -> Result<PtyPair, ApiError> {
    let _opening = OPENING_A_PTY.lock();
    system.openpty(pty_size(size)).map_err(|e| failed("couldn't open a terminal", &e))
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
    front: Front,
    process_records: ProcessRecords,
    /// How often to look at which job the shell runs in front, to record it.
    look_every: Duration,
    /// How long the shell asked to end, and the job it runs, have to end by themselves before they're killed.
    stop_grace: Duration,
    sending: Sending,
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
                self.kill();
            }
        }
        self.process_records.finished();
        ending.forget();
        let exited = self.exit.unwrap_or(TerminalExit { code: None });
        self.sending.finish(TerminalUpdate::Exited(exited)).await;
    }

    async fn until_ended(
        &mut self,
        mirror: &mut Mirror,
        read: &mut mpsc::Receiver<Vec<u8>>,
        told: &mut mpsc::UnboundedReceiver<ToShell>,
        exit: &mut oneshot::Receiver<TerminalExit>,
    ) {
        let (mut kill_at, mut done_at, mut read_all, mut untold) = (None, None, false, false);
        let mut look = tokio::time::interval(self.look_every);
        look.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        while !(read_all && self.exit.is_some()) {
            // By the clock this waits on, so the two agree. Ghostty fails only when it can't allocate, which leaves the
            // copy as it is, uncompressed.
            let compress_at = mirror.compress_at(Instant::now().into_std()).ok().flatten().map(Instant::from_std);
            // In this order, so neither a stop nor the end waits out a program that writes without a pause.
            tokio::select! {
                biased;
                told = told.recv(), if !untold => match told {
                    Some(ToShell::Resize(size)) => self.resize(mirror, size),
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
                    self.kill();
                    kill_at = None;
                }
                // Not on Windows, which has no groups to look at.
                _ = look.tick(), if cfg!(unix) && self.exit.is_none() => self.look_in_front(),
                Ok(written) = self.sending.room().acquire_owned(), if self.sending.holds_any() => {
                    self.sending.send(written).await;
                }
                // Not while it holds as much as it should, so the program waits in its PTY.
                output = read.recv(), if !read_all && !self.sending.full() => match output {
                    Some(output) => self.wrote(mirror, output),
                    None => read_all = true,
                },
                () = until(compress_at) => {
                    let _ = mirror.compress(Instant::now().into_std());
                }
            }
        }
        // The job can outlast the shell, and the PTY, which a Mac closes as the shell ends.
        #[cfg(unix)]
        if let Some(kill_at) = kill_at {
            front::outlasted_by(self.front, kill_at).await;
        }
    }

    /// Asks the shell to end, and the job it runs in front now, and kills them if they haven't by `kill_at`.
    fn hang_up(&mut self, kill_at: &mut Option<Instant>) {
        if kill_at.is_none() && self.exit.is_none() {
            // Before the shell is signalled, since the PTY says which it is only while the shell runs.
            self.look_in_front();
            self.front = Front::HungUpOn(self.front.group());
            self.signal(process::Signal::Hangup);
            *kill_at = Some(Instant::now() + self.stop_grace);
        }
    }

    /// Kills the shell's group, the job it was hung up on, and whatever it runs in front now, such as its exit trap's
    /// command.
    fn kill(&self) {
        // Before the shell is killed, as for a hangup.
        let now = front::in_front(&*self.master).filter(|now| ![self.pid, self.front.group()].contains(&Some(*now)));
        self.signal(process::Signal::Kill);
        if let Some(now) = now {
            process::signal_group(now, process::Signal::Kill);
        }
    }

    fn wrote(&mut self, mirror: &mut Mirror, output: Vec<u8>) {
        mirror.write(&output);
        self.sending.hold(TerminalUpdate::Output { output: TerminalBytes::new(output) });
    }

    /// Gives the worker's copy the new size, and the PTY, which tells the program, and says so in order with the
    /// output. The same size again changes nothing, and so does one either can't take, so the two never differ.
    fn resize(&mut self, mirror: &mut Mirror, size: TerminalSize) {
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
        self.sending.hold(TerminalUpdate::Resized { size });
    }

    fn shown(&mut self, mirror: &Mirror) -> Result<TerminalShown, ApiError> {
        let snapshot = mirror.snapshot().map_err(|e| {
            ApiError::new(ErrorCode::Worker, format!("couldn't say what terminal {} shows: {e}", self.terminal))
        })?;
        let screen = TerminalScreen { size: self.size, bytes: TerminalBytes::new(snapshot) };
        Ok(TerminalShown { screen, next: self.sending.screen_taken() })
    }

    /// Signals the shell's group, only before the shell has ended, since its PID can be another process's after, and
    /// the group of the job it runs in front. A shell that's hung up on sends it on to the jobs it runs behind, so
    /// those end too, but for one started to outlive the terminal, such as with `nohup`.
    fn signal(&self, signal: process::Signal) {
        if let Some(pid) = self.pid.filter(|_| self.exit.is_none()) {
            process::signal_group(pid, signal);
        }
        if let Some(front) = self.front.group() {
            process::signal_group(front, signal);
        }
    }

    /// Notes which job the shell runs in front, but for the shell itself, and records it, since it can outlast the
    /// shell. Once the shell has been hung up on, the job it ran then is the one recorded, as [`Front::HungUpOn`] says.
    fn look_in_front(&mut self) {
        let Front::Looked(_) = self.front else { return };
        let Some(front) = front::in_front(&*self.master) else { return };
        let front = Some(front).filter(|front| Some(*front) != self.pid);
        self.front = Front::Looked(front);
        self.process_records.front(front);
    }
}

/// Never, without a deadline.
#[expect(
    clippy::disallowed_methods,
    reason = "the deadlines a terminal waits for: the end of a stop's grace, when what hasn't ended is killed, of the \
              wait for the last of what its shell wrote, once the shell has ended, and when its history is next compressed"
)]
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
