//! What each crate may depend on, from `docs/architecture.md`'s dependency rules, and that the tool that packages the
//! app is on the version of the crate that reads its packages.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::Value;

use super::workspace::metadata;

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
    for package in ["process", "protocol", "hub", "worker", "agents", "providers", "client-core", "daemon", "cli"] {
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

/// Checks that `package` needs none of our other crates but those in `but`, no async runtime and no UI, so any client
/// on any device can use it.
fn needs_none_of_ours_no_async_runtime_and_no_ui(package: &str, but: &[&str]) {
    let metadata = metadata();
    let closure = runtime_closure(&metadata, package);
    let packages = metadata["packages"].as_array().unwrap();
    let ours: Vec<&str> =
        packages.iter().filter(|p| p["source"].is_null()).filter_map(|p| p["name"].as_str()).collect();
    assert!(ours.contains(&"protocol"), "ours are the packages with no source, which come from paths in the workspace");
    for banned in ours.into_iter().filter(|name| !but.contains(name)).chain(["tokio", "async-std", "smol", "gpui"]) {
        assert!(!closure.contains(banned), "{package} depends on {banned}");
    }
}

/// Dictation is the device's alone, with threads rather than a runtime and no UI, so any client on any device can hear
/// with it. It downloads its model with `fetching`, which is free of the same.
#[test]
fn dictation_needs_none_of_our_other_crates_but_fetching_no_async_runtime_and_no_ui() {
    needs_none_of_ours_no_async_runtime_and_no_ui("dictation", &["fetching"]);
}

/// Fetching knows nothing of what it fetches, and runs on the caller's thread, so any client on any device can fetch
/// with it.
#[test]
fn fetching_needs_none_of_our_other_crates_no_async_runtime_and_no_ui() {
    needs_none_of_ours_no_async_runtime_and_no_ui("fetching", &[]);
}

#[test]
fn the_hub_and_the_worker_stay_apart() {
    let metadata = metadata();
    assert!(!runtime_closure(&metadata, "hub").contains("worker"));
    assert!(!runtime_closure(&metadata, "hub").contains("agents"));
    assert!(!runtime_closure(&metadata, "worker").contains("hub"));
}

/// `vpk` makes the packages and the feed that the app's `velopack` reads, and Velopack releases the two together,
/// so they stay on one version. `tools/package/dotnet-tools.json` pins `vpk`.
#[test]
fn vpk_is_the_velopack_crate_s_version() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    // As `Cargo.lock` has it, which is the version every build takes.
    let lock: toml::Table = std::fs::read_to_string(root.join("Cargo.lock")).unwrap().parse().unwrap();
    let crates: Vec<&str> = lock["package"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| package["name"].as_str() == Some("velopack"))
        .map(|package| package["version"].as_str().unwrap())
        .collect();
    let tools = root.join("tools/package/dotnet-tools.json");
    let tools: Value = serde_json::from_str(&std::fs::read_to_string(tools).unwrap()).unwrap();

    assert_eq!(crates, [tools["tools"]["vpk"]["version"].as_str().unwrap()]);
}
