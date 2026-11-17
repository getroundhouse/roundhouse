//! What the daemon says of its build for the release workflow.

#![cfg(test)]

use process::testing::WholeEnvironment;
use protocol::worker::{WORKER_PROTOCOL_VERSION, previous};

/// The release workflow writes what this prints in the update feed, where the app reads it, so the form is what's
/// tested, as well as the versions.
#[test]
fn the_daemon_says_which_worker_versions_its_hub_speaks() {
    let home = tempfile::tempdir().unwrap();
    let mut command = WholeEnvironment::for_a_test(home.path()).command(env!("CARGO_BIN_EXE_roundhoused"));

    let output = command.arg("worker-versions").output().unwrap();

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let said: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(said, serde_json::json!({"oldest": previous::VERSION, "newest": WORKER_PROTOCOL_VERSION}));
}
