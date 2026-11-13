use std::process::Command;

use crate::sys;

/// A signal sent to a process group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// What a terminal sends as it closes. A shell that gets it sends it on to the jobs it started, and exits.
    Hangup,
    Interrupt,
    Terminate,
    Kill,
}

/// Makes `command` start as the leader of a group of its own, which is what [`signal_group`] needs, so nothing sent
/// to this process's group or terminal reaches it. A `tokio` command takes this through `as_std_mut`.
pub fn own_group(command: &mut Command) {
    sys::own_group(command);
}

/// Signals the group led by `leader`, which has to have been started as a group leader, so a stop reaches
/// everything it started. A PID that would reach every process, or the system's own, is left alone.
///
/// On Windows every signal ends `leader` and everything it started at once, so nothing there gets the chance to
/// stop by itself.
pub fn signal_group(leader: u32, signal: Signal) {
    sys::signal_group(leader, signal);
}

/// Whether any process is left in the group led by `leader`, which can outlast the leader itself. A PID that would ask
/// of every process, or the system's own, has none.
#[cfg(unix)]
pub fn group_running(leader: u32) -> bool {
    sys::group_running(leader)
}

/// Makes `command` start in a session of its own, which it leads, so it outlives this process and its terminal, and
/// nothing sent to their groups reaches it. It leads a process group of its own too, as [`own_group`] would make it.
#[cfg(unix)]
pub fn own_session(command: &mut Command) {
    sys::own_session(command);
}
