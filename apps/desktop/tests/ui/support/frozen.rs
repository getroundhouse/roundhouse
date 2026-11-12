//! A process held still, such as a hub that doesn't answer, for a test of what's on its way to it meanwhile.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::TIMEOUT;

/// A process stopped, as a hub that doesn't answer, until this goes.
pub(crate) struct Frozen(String);

impl Frozen {
    /// Returns once every thread of the process has stopped. Linux hands `SIGSTOP` to one thread, and stops the rest
    /// only once that one runs, so on a busy machine another can still read a request and answer it after `kill`
    /// has returned.
    pub(crate) fn stop(pid: u32) -> Self {
        let frozen = Self(pid.to_string());
        frozen.signal("-STOP");
        let deadline = Instant::now() + TIMEOUT;
        while !stopped(&frozen.0) {
            assert!(Instant::now() < deadline, "process {} never stopped", frozen.0);
            std::thread::sleep(Duration::from_millis(1));
        }
        frozen
    }

    /// Lets the process go on again after `after`, from another thread, which says so just before, so nothing the
    /// process does after can come before it's said.
    pub(crate) fn thaw_after(&self, after: Duration) -> Arc<AtomicBool> {
        let (pid, thawing) = (self.0.clone(), Arc::new(AtomicBool::new(false)));
        let said = thawing.clone();
        std::thread::spawn(move || {
            std::thread::sleep(after);
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
