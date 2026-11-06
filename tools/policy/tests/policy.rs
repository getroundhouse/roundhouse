//! Rules from `CLAUDE.md` and `PROJECT_STRUCTURE.md`, checked against the real dependency graph, so
//! a new dependency that breaks one fails the build rather than waiting to be noticed in review.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::process::Command;

use serde_json::Value;

/// Licenses that are fine to ship in an App Store app.
const ALLOWED: &[&str] = &[
    "0BSD",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "BSL-1.0",
    "CC0-1.0",
    "ISC",
    "MIT",
    "MIT-0",
    "Unicode-3.0",
    "Unlicense",
    "Zlib",
];

fn metadata() -> Value {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked", "--manifest-path", manifest])
        .output()
        .expect("running cargo metadata");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Whether an SPDX expression can be satisfied using only allowed licenses.
fn is_allowed(expression: &str) -> bool {
    // `/` is the old way of writing OR, and still appears in some manifests.
    let normalized = expression.replace('/', " OR ").replace(['(', ')'], " ");
    normalized.split(" OR ").any(|choice| {
        choice
            .split(" AND ")
            .all(|license| ALLOWED.contains(&license.split_whitespace().collect::<Vec<_>>().join(" ").as_str()))
    })
}

#[test]
fn the_license_check_itself_works() {
    assert!(is_allowed("MIT"));
    assert!(is_allowed("MIT OR Apache-2.0"));
    assert!(is_allowed("MIT/Apache-2.0"));
    assert!(is_allowed("(MIT OR Apache-2.0) AND Unicode-3.0"));
    assert!(is_allowed("GPL-3.0-or-later OR Apache-2.0"));
    assert!(!is_allowed("GPL-3.0"));
    assert!(!is_allowed("MIT AND GPL-3.0"));
    assert!(!is_allowed("LGPL-2.1-only OR GPL-3.0"));
}

#[test]
fn nothing_copyleft_is_in_the_tree() {
    let metadata = metadata();
    let mut bad = BTreeMap::new();
    for package in metadata["packages"].as_array().unwrap() {
        // Workspace members have no license field until the project chooses one.
        if package["source"].is_null() {
            continue;
        }
        let name = package["name"].as_str().unwrap();
        match package["license"].as_str() {
            Some(license) if is_allowed(license) => {}
            license => {
                bad.insert(name.to_string(), license.unwrap_or("none declared").to_string());
            }
        }
    }
    assert!(bad.is_empty(), "dependencies with licenses we can't ship: {bad:#?}");
}

/// Everything `package` pulls in, excluding dev-dependencies.
fn runtime_closure(metadata: &Value, package: &str) -> HashSet<String> {
    let id_to_name: HashMap<&str, &str> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p["name"].as_str().unwrap()))
        .collect();
    let nodes: HashMap<&str, &Value> =
        metadata["resolve"]["nodes"].as_array().unwrap().iter().map(|n| (n["id"].as_str().unwrap(), n)).collect();
    let start = id_to_name.iter().find(|(_, name)| **name == package).map(|(id, _)| *id).unwrap();
    let mut seen = HashSet::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        for dep in nodes[id]["deps"].as_array().unwrap() {
            let runtime = dep["dep_kinds"].as_array().unwrap().iter().any(|kind| kind["kind"].is_null());
            let dep_id = dep["pkg"].as_str().unwrap();
            if runtime && seen.insert(id_to_name[dep_id].to_string()) {
                stack.push(dep_id);
            }
        }
    }
    seen
}

#[test]
fn protocol_stays_free_of_io_and_ui() {
    let metadata = metadata();
    let closure = runtime_closure(&metadata, "protocol");
    for banned in ["tokio", "rusqlite", "gpui", "libc"] {
        assert!(!closure.contains(banned), "protocol depends on {banned}");
    }
}

#[test]
fn nothing_outside_the_ui_depends_on_gpui() {
    let metadata = metadata();
    for package in ["protocol", "hub", "worker", "agents", "providers", "client-core", "daemon", "cli"] {
        let gpui: Vec<_> = runtime_closure(&metadata, package)
            .into_iter()
            .filter(|name| name.starts_with("gpui") || name == "zed")
            .collect();
        assert!(gpui.is_empty(), "{package} depends on {gpui:?}");
    }
}

#[test]
fn clients_only_reach_the_hub_through_the_protocol() {
    let metadata = metadata();
    for package in ["client-core", "cli"] {
        let closure = runtime_closure(&metadata, package);
        for banned in ["hub", "worker", "agents", "providers"] {
            assert!(!closure.contains(banned), "{package} depends on {banned}");
        }
    }
}

#[test]
fn the_hub_and_the_worker_stay_apart() {
    let metadata = metadata();
    assert!(!runtime_closure(&metadata, "hub").contains("worker"));
    assert!(!runtime_closure(&metadata, "hub").contains("agents"));
    assert!(!runtime_closure(&metadata, "worker").contains("hub"));
}
