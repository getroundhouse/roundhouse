//! What `cargo metadata` says of the workspace, which each module reads.

use std::process::Command;

use serde_json::Value;

/// The whole dependency graph, on every platform. Cargo holds the lock on its package cache while it works it out, so
/// the tests that read it take turns, and a test that needs only the workspace's own packages reads [`workspace`]
/// instead.
pub(super) fn metadata() -> Value {
    cargo_metadata(&[])
}

/// The workspace's own packages, which Cargo reads without working out the graph or taking its lock.
pub(super) fn workspace() -> Value {
    cargo_metadata(&["--no-deps"])
}

/// The dependency graph as it is on `target`, which [`metadata`] says takes turns with the others.
pub(super) fn metadata_for(target: &str) -> Value {
    cargo_metadata(&["--filter-platform", target])
}

fn cargo_metadata(and: &[&str]) -> Value {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked", "--manifest-path", manifest])
        .args(and)
        .output()
        .expect("running cargo metadata");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}
