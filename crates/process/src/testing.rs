//! What tests of the crates that start processes share.

use std::process::Command;

/// Whether a process is running. One that has exited and that nothing has waited for yet isn't, though it still has
/// its PID.
pub fn is_running(pid: u32) -> bool {
    let Ok(state) = Command::new("ps").args(["-o", "stat=", "-p", &pid.to_string()]).output() else { return false };
    let state = String::from_utf8_lossy(&state.stdout);
    !state.trim().is_empty() && !state.trim().starts_with('Z')
}
