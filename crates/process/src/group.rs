use std::process::Command;

use crate::sys;

/// A signal sent to a process group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
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

/// Makes `command` start apart from this process, so it outlives this process, and closing this one's terminal or
/// console doesn't reach it.
///
/// On Unix it starts in a session of its own, which it leads, so nothing sent to this process's group or its
/// terminal's reaches it, and it leads a process group of its own too, as [`own_group`] would make it. On Windows it
/// gets a console of its own, with no window: it needs one, or everything it starts would open a console window each,
/// where this way they share its hidden one. And there [`keep_stdio_from_children`](crate::keep_stdio_from_children)
/// runs first, or it would hold this process's own input and output for as long as it runs. That keeps them from
/// everything this process starts from then on, not only from `command`.
pub fn detach(command: &mut Command) {
    sys::detach(command);
}
