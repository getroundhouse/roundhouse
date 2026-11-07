//! Rules from `AGENTS.md`, `docs/architecture.md` and `docs/rust.md`, checked against the real
//! dependency graph and source tree, so breaking one fails the build rather than waiting to be noticed
//! in review.

#![cfg(test)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
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

/// The most lines a Rust file can have, tests included. `docs/rust.md` has the reasoning.
const MAX_FILE_LINES: usize = 400;

/// Files allowed past that limit, as (path from the repo root, why). Empty so far.
const LONG_FILES: &[(&str, &str)] = &[];

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

/// What ends up in a binary on one platform: its dependencies, leaving out build tools and proc macros,
/// which only run while compiling.
fn shipped_packages(metadata: &Value, root: &str) -> BTreeMap<String, String> {
    let packages: HashMap<&str, &Value> =
        metadata["packages"].as_array().unwrap().iter().map(|p| (p["id"].as_str().unwrap(), p)).collect();
    let nodes: HashMap<&str, &Value> =
        metadata["resolve"]["nodes"].as_array().unwrap().iter().map(|n| (n["id"].as_str().unwrap(), n)).collect();
    let is_proc_macro = |package: &Value| {
        package["targets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|target| target["kind"].as_array().unwrap().iter().any(|kind| kind == "proc-macro"))
    };
    let start = packages.iter().find(|(_, p)| p["name"] == root).map(|(id, _)| *id).expect("the root package");
    let mut shipped = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        for dep in nodes[id]["deps"].as_array().unwrap() {
            let dep_id = dep["pkg"].as_str().unwrap();
            let linked = dep["dep_kinds"].as_array().unwrap().iter().any(|kind| kind["kind"].is_null());
            if !linked || is_proc_macro(packages[dep_id]) || !seen.insert(dep_id) {
                continue;
            }
            let package = packages[dep_id];
            // Workspace members have no license field until the project chooses one.
            if !package["source"].is_null() {
                let license = package["license"].as_str().unwrap_or("none declared").to_string();
                shipped.insert(package["name"].as_str().unwrap().to_string(), license);
            }
            stack.push(dep_id);
        }
    }
    shipped
}

fn metadata_for(target: &str) -> Value {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--filter-platform",
            target,
            "--manifest-path",
            manifest,
        ])
        .output()
        .expect("running cargo metadata");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Checks what ships, on the platforms it ships on, rather than everything in `Cargo.lock`. That also
/// holds build tools and other platforms' crates, some of them MPL-2.0: `cbindgen` (a build tool),
/// `dwrote` (Windows only) and `option-ext` (only reached through a proc macro). Windows and iOS are
/// here so that adding either has to face this.
#[test]
fn nothing_copyleft_ships() {
    for (root, targets) in [
        ("desktop", &["aarch64-apple-darwin", "x86_64-apple-darwin"][..]),
        (
            "daemon",
            &["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"][..],
        ),
        (
            "cli",
            &["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"][..],
        ),
    ] {
        for target in targets {
            let shipped = shipped_packages(&metadata_for(target), root);
            assert!(!shipped.is_empty(), "{root} on {target} should have dependencies");
            let bad: BTreeMap<_, _> = shipped.into_iter().filter(|(_, license)| !is_allowed(license)).collect();
            assert!(bad.is_empty(), "{root} on {target} would ship licenses we can't: {bad:#?}");
        }
    }
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

#[test]
fn the_hub_and_the_worker_stay_apart() {
    let metadata = metadata();
    assert!(!runtime_closure(&metadata, "hub").contains("worker"));
    assert!(!runtime_closure(&metadata, "hub").contains("agents"));
    assert!(!runtime_closure(&metadata, "worker").contains("hub"));
}

/// The directory of each crate the Rust standards cover. `third_party/` is exempt, because it mirrors
/// another crate's API.
fn covered_crates(metadata: &Value) -> Vec<PathBuf> {
    let members: HashSet<&str> =
        metadata["workspace_members"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
    let third_party = Path::new(metadata["workspace_root"].as_str().unwrap()).join("third_party");
    metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| members.contains(package["id"].as_str().unwrap()))
        .map(|package| Path::new(package["manifest_path"].as_str().unwrap()).parent().unwrap().to_path_buf())
        .filter(|dir| !dir.starts_with(&third_party))
        .collect()
}

fn add_rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            add_rust_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn every_file_stays_within_the_length_limit() {
    let metadata = metadata();
    let root = Path::new(metadata["workspace_root"].as_str().unwrap());
    let mut files = Vec::new();
    for dir in covered_crates(&metadata) {
        add_rust_files(&dir, &mut files);
    }
    assert!(files.len() > 100, "found only {} files, so this isn't looking in the right place", files.len());
    let long: BTreeMap<String, usize> = files
        .iter()
        .map(|file| (file, std::fs::read_to_string(file).unwrap().lines().count()))
        .filter(|(_, lines)| *lines > MAX_FILE_LINES)
        .map(|(file, lines)| (file.strip_prefix(root).unwrap().display().to_string(), lines))
        .collect();
    let unexplained: Vec<_> =
        long.iter().filter(|(path, _)| !LONG_FILES.iter().any(|(file, _)| file == path)).collect();
    assert!(unexplained.is_empty(), "over {MAX_FILE_LINES} lines, so split by responsibility: {unexplained:?}");
    let stale: Vec<_> = LONG_FILES.iter().filter(|(file, _)| !long.contains_key(*file)).collect();
    assert!(stale.is_empty(), "within the limit now, so take these off `LONG_FILES`: {stale:?}");
}

#[test]
fn every_crate_inherits_the_lints() {
    let metadata = metadata();
    let crates = covered_crates(&metadata);
    assert!(crates.len() > 10, "found only {} crates, so this isn't looking in the right place", crates.len());
    let missing: Vec<_> = crates
        .iter()
        .filter(|dir| {
            // Cargo's metadata leaves out `[lints]`, so this reads the manifest.
            let manifest: toml::Table = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap().parse().unwrap();
            manifest.get("lints").and_then(|lints| lints.get("workspace")).and_then(toml::Value::as_bool) != Some(true)
        })
        .collect();
    assert!(missing.is_empty(), "these need `[lints] workspace = true`: {missing:?}");
}
