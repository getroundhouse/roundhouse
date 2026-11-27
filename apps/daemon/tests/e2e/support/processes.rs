//! Signalling the processes a stack runs, asking about them, and stopping its hub.

use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::process::Child;
#[cfg(unix)]
use std::process::Command;
use std::time::Duration;

use super::Stack;

/// Returns what `kill` did. Signal 0 only checks that the process exists.
#[cfg(unix)]
pub(crate) fn signal(pid: u32, signal: i32) -> i32 {
    // SAFETY: `kill` takes any PID and signal, and touches no memory of ours.
    unsafe { libc::kill(i32::try_from(pid).unwrap(), signal) }
}

#[cfg(unix)]
pub(crate) fn terminate(pid: u32) {
    signal(pid, libc::SIGTERM);
}

pub(crate) use process::testing::is_running as is_alive;

#[cfg(unix)]
pub(crate) fn parent_of(pid: u32) -> u32 {
    let out = Command::new("ps").args(["-o", "ppid=", "-p", &pid.to_string()]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

/// Asks `hub`, at `socket`, to stop: with SIGTERM, as a service manager asks it.
#[cfg(unix)]
fn ask_to_stop(hub: &Hub, _socket: &Path) {
    terminate(hub.id());
}

/// Asks `hub`, at `socket`, to stop: with `hub stop` on its pipe, since Windows sends Ctrl-Break, its nearest to
/// SIGTERM, only to a process on the sender's own console.
#[cfg(windows)]
fn ask_to_stop(_hub: &Hub, socket: &Path) {
    let _ = client_core::stop_hub(socket);
}

/// A hub a stack started. Dropped while it's still running, it's killed, and waited for, so it's never left behind,
/// such as by a test that took it from the stack to see it exit, and failed first. Its workers and agents go as the
/// stack is dropped, after it, by what they recorded.
pub(crate) struct Hub(Child);

impl From<Child> for Hub {
    fn from(child: Child) -> Self {
        Self(child)
    }
}

impl Deref for Hub {
    type Target = Child;

    fn deref(&self) -> &Child {
        &self.0
    }
}

impl DerefMut for Hub {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

impl Stack {
    /// As a hub that crashes goes, leaving its workers running for the next hub, which carries on with their agents.
    pub(crate) fn kill_hub(&mut self) {
        let mut hub = self.hub.take().unwrap();
        hub.kill().unwrap();
        hub.wait().unwrap();
    }

    /// Kills the hub, as a crash would, and starts another on the same data, which carries on with what it left.
    pub(crate) fn restart_hub(&mut self) {
        self.kill_hub();
        self.start_hub();
    }

    /// As when the machine goes down: the hub, and its workers and their agents with it, so the next hub finds the
    /// agents gone.
    pub(crate) fn kill_hub_and_workers(&mut self) {
        self.kill_hub();
        self.kill_what_hubs_left_running();
    }

    /// Asks it to stop, which stops its workers and their agents, so the next hub starts the next agent afresh, and
    /// waits for it to exit, or kills it, as dropping it does, if it takes too long.
    pub(crate) fn stop_hub(&mut self) {
        let Some(mut hub) = self.hub.take() else { return };
        ask_to_stop(&hub, &self.socket());
        waiting::thread::up_to(Duration::from_secs(10), || !matches!(hub.try_wait(), Ok(None)));
    }

    /// Each instance's worker, and the agents it recorded, with no hub to stop them, whichever provider has it.
    fn kill_what_hubs_left_running(&self) {
        let adapters = std::fs::read_dir(self.path("adapter-instances")).into_iter().flatten();
        let local = std::fs::read_dir(self.paths().instances_dir).into_iter().flatten();
        for instance in local.chain(adapters).flatten() {
            let agents = std::fs::read_dir(instance.path().join("agents")).into_iter().flatten().flatten();
            let worker = instance.path().join(protocol::paths::WORKER_RECORD_FILE);
            for record in std::iter::once(worker).chain(agents.map(|agent| agent.path())) {
                if let Some(running) = process::ProcessRecord::read(&record).filter(process::ProcessRecord::is_running)
                {
                    process::signal_group(running.pid, process::Signal::Kill);
                }
            }
        }
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        // For a test that failed, since what it waited for and never saw is often the hub's to explain, and nothing of
        // a stack outlasts its test but what the test printed.
        if std::thread::panicking() {
            let logged = std::fs::read(self.hub_log()).unwrap_or_default();
            eprintln!("what the stack's hubs logged:\n{}", String::from_utf8_lossy(&logged));
        }
        self.stop_hub();
        // A hub that was killed left its workers running for the next hub, and there's no next one.
        self.kill_what_hubs_left_running();
    }
}
