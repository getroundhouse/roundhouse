//! Whether a process a test started is still running.

use std::process::Command;

/// Whether a process is running. One that has exited and that nothing has waited for yet isn't, though it still has
/// its PID.
///
/// # Panics
///
/// If `ps` can't be run, rather than answering that it isn't, which a test waiting for a process to go would take for
/// what it waited for.
pub fn is_running(pid: u32) -> bool {
    let state = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap_or_else(|e| panic!("couldn't run ps to see whether {pid} is running: {e}"));
    let state = String::from_utf8_lossy(&state.stdout);
    !state.trim().is_empty() && !state.trim().starts_with('Z')
}
