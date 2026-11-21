//! What `cargo metadata` and `cargo tree` say of the workspace, which each module reads.

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
    let output = cargo(&["metadata", "--format-version", "1"], and);
    serde_json::from_slice(&output).unwrap()
}

/// The packages a build of `package` takes, on every platform, without its dev-dependencies and with the features
/// that build turns on, which [`metadata`] can't say, since it works out the features of every package at once, tests
/// and all.
pub(super) fn built_by(package: &str) -> Vec<String> {
    let output = cargo(
        &["tree", "--package", package, "--edges", "normal"],
        &["--target", "all", "--prefix", "none", "--format", "{p}"],
    );
    let tree = String::from_utf8(output).unwrap();
    tree.lines().filter_map(|line| line.split_whitespace().next()).map(str::to_string).collect()
}

/// What Cargo writes for `command`, on the workspace's lock, with `and` after it.
fn cargo(command: &[&str], and: &[&str]) -> Vec<u8> {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args(command)
        .args(["--locked", "--manifest-path", manifest])
        .args(and)
        .output()
        .expect("running cargo");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    output.stdout
}
