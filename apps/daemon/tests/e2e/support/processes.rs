//! Signalling the processes a stack runs, and asking about them.

use std::process::Command;

/// Returns what `kill` did. Signal 0 only checks that the process exists.
pub(crate) fn signal(pid: u32, signal: i32) -> i32 {
    // SAFETY: `kill` takes any PID and signal, and touches no memory of ours.
    unsafe { libc::kill(i32::try_from(pid).unwrap(), signal) }
}

pub(crate) fn terminate(pid: u32) {
    signal(pid, libc::SIGTERM);
}

pub(crate) use process::testing::is_running as is_alive;

pub(crate) fn parent_of(pid: u32) -> u32 {
    let out = Command::new("ps").args(["-o", "ppid=", "-p", &pid.to_string()]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}
