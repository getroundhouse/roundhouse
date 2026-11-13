//! Keeping this machine awake while agents work, so it doesn't go to sleep by itself partway through a turn. A program
//! does it for as long as it runs, and this is what starts it, watches it and stops it.

use std::fmt;
use std::future::pending;
use std::io;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::warn;

use super::Hub;
use timing::{Delays, Step, Timing};

#[cfg(test)]
mod tests;
mod timing;

const DELAYS: Delays = Delays {
    // Work often stops for a moment: the hub sends a queued prompt just after a turn ends, and an agent picks up again
    // just after what it left in the background finishes. Nothing says how soon a machine that's been left alone for
    // longer than its timer sleeps once nothing stops it, so it's kept awake across those, at the cost of this much
    // longer awake after the last of the work.
    linger: Duration::from_secs(30),
    // Far shorter than any machine's timer for going to sleep, so one that something else stopped is back long before
    // it's missed.
    retry: Duration::from_secs(5),
};

/// What keeping this machine awake goes by, besides the work itself, and whether it's being kept awake now.
pub(super) struct Awake {
    /// Notified when what's at work may have changed, or whether the machine is to be kept awake at all.
    changed: Notify,
    /// Whether to keep the machine awake while there's work, as the hub's settings say.
    wanted: AtomicBool,
    /// Whether the program that keeps it awake is running.
    holding: AtomicBool,
}

impl Default for Awake {
    fn default() -> Self {
        Self { changed: Notify::new(), wanted: AtomicBool::new(true), holding: AtomicBool::new(false) }
    }
}

impl Awake {
    /// Says that what's at work may have changed.
    pub(super) fn work_changed(&self) {
        self.changed.notify_one();
    }

    /// Whether to keep the machine awake while there's work, from now on.
    pub(super) fn want(&self, on: bool) {
        self.wanted.store(on, Ordering::SeqCst);
        self.changed.notify_one();
    }

    pub(super) fn holding(&self) -> bool {
        self.holding.load(Ordering::SeqCst)
    }

    fn hold(&self, at_work: bool) -> Hold {
        if !self.wanted.load(Ordering::SeqCst) {
            Hold::LetGoNow
        } else if at_work {
            Hold::Keep
        } else {
            Hold::LetGoSoon
        }
    }
}

/// Whether to keep the machine awake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hold {
    /// There's work.
    Keep,
    /// The work has stopped, which may only be for a moment.
    LetGoSoon,
    /// Nobody wants it kept awake, whatever is at work.
    LetGoNow,
}

/// A program that keeps this machine from going to sleep by itself for as long as it runs.
struct Stimulant {
    program: PathBuf,
    args: Vec<String>,
}

impl Stimulant {
    /// This machine's, if it comes with one. Only a Mac does. A Linux server doesn't sleep by itself.
    fn for_this_machine() -> Option<Self> {
        // `-i` rules out only the sleep that comes from nobody using the machine. The display still sleeps, and so
        // does the machine when its lid closes or its battery runs low. `-w` has it stop when this hub goes,
        // however that happens, so a hub that's killed doesn't leave the machine awake.
        let args = ["-i", "-w", &std::process::id().to_string()].map(str::to_string).to_vec();
        cfg!(target_os = "macos").then(|| Self { program: PathBuf::from("/usr/bin/caffeinate"), args })
    }

    /// Stopped when what's returned is dropped, so it can't outlive whatever is watching it.
    fn start(&self) -> io::Result<Child> {
        // The hub's own output is its log, which this has nothing to say in.
        Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
    }
}

impl Hub {
    /// Whether any agent is doing something that goes on without a person, or a workspace is being set up.
    fn at_work(&self) -> bool {
        !self.setup_gates.lock().is_empty()
            || self.sessions.lock().live.values().any(|session| session.works_unattended())
    }

    /// Keeps this machine awake while there's work, until what's returned is aborted. Nothing to do where the
    /// machine has no way to be kept awake.
    pub(crate) fn keep_awake(self: &Arc<Self>) -> Option<JoinHandle<()>> {
        let stimulant = Stimulant::for_this_machine()?;
        let hub = Arc::downgrade(self);
        let at_work = move || hub.upgrade().is_some_and(|hub| hub.at_work());
        Some(tokio::spawn(hold_while(self.awake.clone(), at_work, stimulant, DELAYS)))
    }
}

/// Runs `stimulant` while `awake` wants the machine kept awake and `at_work` says there's work, as [`Timing`] has it.
/// It looks again each time `awake` is told of a change, and starts the program again if it stops by itself. Only
/// this starts and stops the program, so two changes at once can't leave it running with no work, or stopped with
/// some.
async fn hold_while(awake: Arc<Awake>, at_work: impl Fn() -> bool, stimulant: Stimulant, delays: Delays) {
    let mut held: Option<Child> = None;
    let mut timing = Timing::default();
    let mut trouble = Trouble::default();
    loop {
        match timing.step(awake.hold(at_work()), held.is_some(), Instant::now(), delays) {
            Step::Start => match stimulant.start() {
                Ok(child) => held = Some(child),
                Err(e) => {
                    timing.failed(Instant::now(), delays);
                    trouble.say(&stimulant, &format_args!("it couldn't start: {e}"));
                }
            },
            Step::Stop => {
                if let Some(mut child) = held.take() {
                    // Waits for it too, so nothing is left of it.
                    let _ = child.kill().await;
                }
                trouble = Trouble::default();
            }
            Step::Nothing => {}
        }
        awake.holding.store(held.is_some(), Ordering::SeqCst);
        tokio::select! {
            () = awake.changed.notified() => {}
            () = until(timing.next_step_at()) => {}
            exit = exited(&mut held) => {
                held = None;
                timing.failed(Instant::now(), delays);
                match exit {
                    Ok(status) => trouble.say(&stimulant, &format_args!("it stopped by itself, with {status}")),
                    Err(e) => trouble.say(&stimulant, &format_args!("it can't be watched: {e}")),
                }
            }
        }
    }
}

/// Waits until `at`, or for good if there's nothing to wait for.
async fn until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => pending().await,
    }
}

/// Waits for the program to stop by itself, or for good if it isn't running.
async fn exited(held: &mut Option<Child>) -> io::Result<ExitStatus> {
    match held {
        Some(child) => child.wait().await,
        None => pending().await,
    }
}

/// Whether the log has been told that the program isn't staying up, since it was last stopped on purpose. Once is
/// enough, however many times it's started again.
#[derive(Default)]
struct Trouble {
    said: bool,
}

impl Trouble {
    fn say(&mut self, stimulant: &Stimulant, problem: &dyn fmt::Display) {
        if !std::mem::replace(&mut self.said, true) {
            let program = stimulant.program.display();
            warn!("{program} isn't keeping this machine awake, and will be started again: {problem}");
        }
    }
}
