//! A process held still, such as a hub that doesn't answer, for a test of what's on its way to it meanwhile, and
//! quitting given less time to wait for it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui_kit::AppContext as _;

use super::{Stack, TIMEOUT, Ui, sidebar_loaded};

/// A process stopped, as a hub that doesn't answer, until this goes.
pub(crate) struct Frozen(String);

impl Frozen {
    /// Returns once every thread of the process has stopped. Linux hands `SIGSTOP` to one thread, and stops the rest
    /// only once that one runs, so on a busy machine another can still read a request and answer it after `kill`
    /// has returned.
    pub(crate) fn stop(pid: u32) -> Self {
        let frozen = Self(pid.to_string());
        frozen.signal("-STOP");
        waiting::thread::until(TIMEOUT, format!("process {} never stopped", frozen.0), || stopped(&frozen.0));
        frozen
    }

    /// Lets the process go on again after `after`, from another thread, which says so just before, so nothing the
    /// process does after can come before it's said. Nothing says the app has started waiting for the process, so
    /// `after` is the time the app has to do what a test checks it doesn't do while it waits, such as quit.
    pub(crate) fn thaw_after(&self, after: Duration) -> Arc<AtomicBool> {
        let (pid, thawing) = (self.0.clone(), Arc::new(AtomicBool::new(false)));
        let said = thawing.clone();
        std::thread::spawn(move || {
            waiting::thread::for_what_mustnt_happen(after);
            said.store(true, Ordering::SeqCst);
            std::process::Command::new("kill").args(["-CONT", &pid]).status().unwrap();
        });
        thawing
    }

    fn signal(&self, signal: &str) {
        let sent = std::process::Command::new("kill").args([signal, &self.0]).status().unwrap();
        assert!(sent.success(), "kill {signal} {}", self.0);
    }
}

impl Drop for Frozen {
    fn drop(&mut self) {
        self.signal("-CONT");
    }
}

/// Whether every thread of the process has stopped, or is gone, as each one's `stat` says: its state follows its name,
/// which is in parentheses and can hold anything, so it's read from after the last of them.
#[cfg(target_os = "linux")]
fn stopped(pid: &str) -> bool {
    let Ok(threads) = std::fs::read_dir(format!("/proc/{pid}/task")) else { return false };
    threads.flatten().all(|thread| {
        let stat = std::fs::read_to_string(thread.path().join("stat")).unwrap_or_default();
        stat.rsplit_once(") ").is_some_and(|(_, after)| after.starts_with(['T', 't', 'Z', 'X']))
    })
}

/// Whether the process has stopped, as `ps` says, which on a Mac is once all of it has.
#[cfg(not(target_os = "linux"))]
fn stopped(pid: &str) -> bool {
    let shown = std::process::Command::new("ps").args(["-o", "stat=", "-p", pid]).output().unwrap();
    String::from_utf8_lossy(&shown.stdout).trim_start().starts_with('T')
}

/// How long quitting waits for a frozen hub, in a test whose hub is held still until quitting has given up on it. The
/// hub can't answer however long it's given, so this only has to be long enough to be a wait.
const GIVE_UP_AFTER: Duration = Duration::from_millis(250);

impl Ui<'_> {
    /// Holds the stack's hub still until what's returned goes, as [`Frozen::stop`] does, once the workspace on screen
    /// has read what it reads by itself, as [`Self::wait_for_the_sidebar_of_changes`] says.
    pub(crate) fn freeze_hub(&mut self, stack: &Stack) -> Frozen {
        self.wait_for_the_sidebar_of_changes();
        Frozen::stop(stack.client().hub_status().unwrap().pid)
    }

    /// Waits until the workspace on screen, if there is one, shows what its sidebar of changes starts on, for a test
    /// about to hold its hub still. The sidebar lists the workspace's files once the hub's summary of its changes says
    /// there are none, and the summary comes by itself, a moment after the workspace does. In a test the app's reads
    /// run on the test's own thread, so a list asked of a hub held still would wait for good, and the test with it.
    pub(crate) fn wait_for_the_sidebar_of_changes(&mut self) {
        self.wait("the sidebar of changes to show what it starts on", |ui| {
            let Some(pane) = ui.pane() else { return true };
            ui.cx.read_entity(&pane, |pane, cx| !pane.changes_open(cx) || sidebar_loaded(pane, cx))
        });
    }

    /// Has quitting, and restarting to update, give up on what's on its way to a hub after [`GIVE_UP_AFTER`], rather
    /// than the app's own wait, for a test whose hub is held still until after they've given up on it.
    pub(crate) fn give_up_on_a_frozen_hub_soon(&mut self) {
        self.cx.update(|cx| cx.set_global(desktop::Timers { keeping: GIVE_UP_AFTER, ..desktop::Timers::DEFAULT }));
    }
}
