//! What the daemon says of its build for the release workflow.

#![cfg(test)]

use std::process::Command;

use protocol::worker::{WORKER_PROTOCOL_VERSION, previous};

/// The release workflow writes what this prints in the update feed, where the app reads it, so the form is what's
/// tested, as well as the versions.
#[test]
fn the_daemon_says_which_worker_versions_its_hub_speaks() {
    let output = Command::new(env!("CARGO_BIN_EXE_roundhoused")).arg("worker-versions").output().unwrap();

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let said: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(said, serde_json::json!({"oldest": previous::VERSION, "newest": WORKER_PROTOCOL_VERSION}));
}
