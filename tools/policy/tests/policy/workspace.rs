//! What `cargo metadata` and `cargo tree` say of the workspace, which each module reads.

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use serde_json::Value;

/// The whole dependency graph, on every platform, read once for every test of a run. Cargo holds the lock on its
/// package cache while it works a graph out, so the tests that read one take turns, and when each test that reads this
/// read it for itself, the last waited behind every other. A test that needs only the workspace's own packages reads
/// [`workspace`] instead.
pub(super) fn metadata() -> &'static Value {
    static READ: OnceLock<Value> = OnceLock::new();
    READ.get_or_init(|| {
        // nextest starts a process for each test, so the first to ask reads it and keeps it here for the rest.
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
        let kept = dir.join("policy-metadata.json");
        let mut read = None;
        process::testing::once_per_run(dir, "policy-metadata", || {
            let output = cargo_metadata(&[]);
            std::fs::write(&kept, &output).unwrap();
            read = Some(output);
        });
        let output = read.unwrap_or_else(|| std::fs::read(&kept).unwrap());
        serde_json::from_slice(&output).unwrap()
    })
}

/// The workspace's own packages, which Cargo reads without working out the graph or taking its lock.
pub(super) fn workspace() -> Value {
    serde_json::from_slice(&cargo_metadata(&["--no-deps"])).unwrap()
}

/// The dependency graph as it is on `target`, which [`metadata`] says takes turns with the others.
pub(super) fn metadata_for(target: &str) -> Value {
    serde_json::from_slice(&cargo_metadata(&["--filter-platform", target])).unwrap()
}

fn cargo_metadata(and: &[&str]) -> Vec<u8> {
    cargo(&["metadata", "--format-version", "1"], and)
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
