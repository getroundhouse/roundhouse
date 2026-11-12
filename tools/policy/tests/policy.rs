//! Rules from `AGENTS.md`, `docs/architecture.md` and `docs/rust.md`, checked against the real
//! dependency graph and source tree, so breaking one fails the build rather than waiting to be noticed
//! in review.

#![cfg(test)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// Licenses that are fine in anything that ships, App Store apps included. `docs/architecture.md` has why
/// these, and why not GPL, AGPL or LGPL.
const ALLOWED: &[&str] = &[
    "0BSD",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "BSL-1.0",
    "CC0-1.0",
    // A license for data, not code, which only `webpki-roots` uses here: Mozilla's root certificates, which the
    // update feed's HTTPS client trusts. Its one condition is shipping the license's text with the data.
    "CDLA-Permissive-2.0",
    "ISC",
    "MIT",
    "MIT-0",
    "Unicode-3.0",
    "Unlicense",
    "Zlib",
];

/// File-level copyleft. Only the crate's own files stay under it, and shipping them unchanged just means
/// saying where their source is. A changed copy would have to be published, so these are only allowed
/// straight from crates.io, where a patched or vendored copy shows up as a different source.
const ALLOWED_UNCHANGED: &[&str] = &["MPL-2.0"];

/// How `cargo metadata` names crates.io as a package's source.
const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// Someone else's files kept in this repo rather than taken as a crate, as (the folder, its SPDX license, words its
/// `LICENSE` has to hold). The dependency graph doesn't show them, so the license check can't see them unless they're
/// listed here.
const VENDORED: &[(&str, &str, &str)] = &[
    // Catppuccin's icons, manifest and colours, which `tools/file-icons/update.py` writes.
    ("apps/desktop/file-icons", "MIT", "Permission is hereby granted, free of charge"),
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

/// Whether a package with this SPDX license expression can ship, given where it comes from. `source` is
/// `None` for a package kept in this repo.
fn may_ship(expression: &str, source: Option<&str>) -> bool {
    let unchanged = source == Some(CRATES_IO);
    // `/` is the old way of writing OR, and still appears in some manifests.
    let normalized = expression.replace('/', " OR ").replace(['(', ')'], " ");
    normalized.split(" OR ").any(|choice| {
        choice.split(" AND ").all(|license| {
            let license = license.split_whitespace().collect::<Vec<_>>().join(" ");
            ALLOWED.contains(&license.as_str()) || (unchanged && ALLOWED_UNCHANGED.contains(&license.as_str()))
        })
    })
}

#[test]
fn the_license_check_itself_works() {
    let git = Some("git+https://github.com/example/fork");
    assert!(may_ship("MIT", Some(CRATES_IO)));
    assert!(may_ship("MIT OR Apache-2.0", Some(CRATES_IO)));
    assert!(may_ship("MIT/Apache-2.0", Some(CRATES_IO)));
    assert!(may_ship("(MIT OR Apache-2.0) AND Unicode-3.0", Some(CRATES_IO)));
    assert!(may_ship("GPL-3.0-or-later OR Apache-2.0", Some(CRATES_IO)));
    assert!(may_ship("MIT", git));
    assert!(may_ship("MIT", None));
    assert!(!may_ship("GPL-3.0", Some(CRATES_IO)));
    assert!(!may_ship("MIT AND GPL-3.0", Some(CRATES_IO)));
    assert!(!may_ship("LGPL-2.1-only OR GPL-3.0", Some(CRATES_IO)));
    assert!(!may_ship("AGPL-3.0-only", Some(CRATES_IO)));

    // MPL-2.0 only straight from crates.io.
    assert!(may_ship("MPL-2.0", Some(CRATES_IO)));
    assert!(may_ship("MIT AND MPL-2.0", Some(CRATES_IO)));
    assert!(!may_ship("MPL-2.0", git));
    assert!(!may_ship("MPL-2.0", None));
    assert!(!may_ship("MIT AND MPL-2.0", git));
    assert!(!may_ship("MPL-2.0 AND GPL-3.0", Some(CRATES_IO)));
}

/// What ends up in a binary on one platform: its own package and its dependencies, leaving out build tools and
/// proc macros, which only run while compiling. Each maps to its license expression and its source.
fn shipped_packages(metadata: &Value, root: &str) -> BTreeMap<String, (String, Option<String>)> {
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
    let mut seen = HashSet::from([start]);
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        // The project's own crates declare its license like any other, so one that declares none can't ship.
        let package = packages[id];
        let license = package["license"].as_str().unwrap_or("none declared").to_string();
        let source = package["source"].as_str().map(str::to_string);
        shipped.insert(package["name"].as_str().unwrap().to_string(), (license, source));
        for dep in nodes[id]["deps"].as_array().unwrap() {
            let dep_id = dep["pkg"].as_str().unwrap();
            let linked = dep["dep_kinds"].as_array().unwrap().iter().any(|kind| kind["kind"].is_null());
            if linked && !is_proc_macro(packages[dep_id]) && seen.insert(dep_id) {
                stack.push(dep_id);
            }
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

/// Checks what ships, on the platforms it ships on, rather than everything in `Cargo.lock`, which also
/// holds build tools, proc macros and other platforms' crates. Add each platform here as it starts
/// shipping, such as iOS.
#[test]
fn only_allowed_licenses_ship() {
    const MAC: [&str; 2] = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
    const LINUX: [&str; 2] = ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"];
    const WINDOWS: [&str; 2] = ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"];
    for (root, targets) in [
        ("desktop", &[MAC[0], MAC[1], WINDOWS[0], WINDOWS[1]][..]),
        ("daemon", &[MAC[0], MAC[1], LINUX[0], LINUX[1], WINDOWS[0], WINDOWS[1]][..]),
        ("cli", &[MAC[0], MAC[1], LINUX[0], LINUX[1], WINDOWS[0], WINDOWS[1]][..]),
    ] {
        for target in targets {
            let shipped = shipped_packages(&metadata_for(target), root);
            assert!(shipped.len() > 1, "{root} on {target} should have dependencies");
            let bad: BTreeMap<_, _> =
                shipped.into_iter().filter(|(_, (license, source))| !may_ship(license, source.as_deref())).collect();
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

/// `vpk` makes the packages and the feed that the app's `velopack` reads, and Velopack releases the two together,
/// so they stay on one version. `tools/package/dotnet-tools.json` pins `vpk`.
#[test]
fn vpk_is_the_velopack_crate_s_version() {
    let metadata = metadata();
    let crates: Vec<&str> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| package["name"] == "velopack")
        .map(|package| package["version"].as_str().unwrap())
        .collect();
    let tools = Path::new(metadata["workspace_root"].as_str().unwrap()).join("tools/package/dotnet-tools.json");
    let tools: Value = serde_json::from_str(&std::fs::read_to_string(tools).unwrap()).unwrap();

    assert_eq!(crates, [tools["tools"]["vpk"]["version"].as_str().unwrap()]);
}

/// The directory of each crate the Rust standards cover, which is every workspace member.
fn covered_crates(metadata: &Value) -> Vec<PathBuf> {
    let members: HashSet<&str> =
        metadata["workspace_members"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
    metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| members.contains(package["id"].as_str().unwrap()))
        .map(|package| Path::new(package["manifest_path"].as_str().unwrap()).parent().unwrap().to_path_buf())
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

/// Folders `.gitignore` leaves out, which can hold what's installed or built, licenses and all. Hidden ones are left
/// out too.
const NOT_IN_THE_REPO: &[&str] = &["target", "node_modules", "dist"];

/// Whether a file's name says it's a license, as `LICENSE`, `LICENSE-MIT`, `LICENSE.md`, `COPYING.LESSER` or `NOTICE`
/// do, and `license.svg`, a picture, doesn't.
fn is_license(name: &str) -> bool {
    let name = name.to_lowercase();
    let (stem, extension) = name.split_once('.').unwrap_or((&name, ""));
    let kinds = ["license", "licence", "copying", "notice", "unlicense"];
    let named = kinds
        .iter()
        .any(|kind| stem.strip_prefix(kind).is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '_'])));
    named && ["", "md", "txt", "rst", "lesser", "lib"].contains(&extension)
}

/// The folder of every license file under `dir`, from `root`, leaving out links and what isn't in the repo.
fn license_folders(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) {
    // A folder that's gone by the time it's read, such as one `tools/file-icons/update.py` works in, holds nothing.
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() && !name.starts_with('.') && !NOT_IN_THE_REPO.contains(&name.as_str()) {
            license_folders(root, &entry.path(), found);
        } else if kind.is_file() && is_license(&name) {
            found.push(dir.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

#[test]
fn a_license_is_known_by_its_name() {
    for name in ["LICENSE", "LICENSE-MIT", "LICENSE_APACHE.txt", "LICENSE.md", "COPYING.LESSER", "NOTICE", "UNLICENSE"]
    {
        assert!(is_license(name), "{name}");
    }
    for name in ["license.svg", "licenses.md", "notices.rs", "README.md"] {
        assert!(!is_license(name), "{name}");
    }
}

#[test]
fn files_kept_from_elsewhere_ship_with_an_allowed_license() {
    let metadata = metadata();
    let root = Path::new(metadata["workspace_root"].as_str().unwrap());
    for (dir, license, words) in VENDORED {
        assert!(ALLOWED.contains(license), "{dir} is under {license}, which can't ship");
        let text = std::fs::read_to_string(root.join(dir).join("LICENSE")).unwrap_or_default();
        assert!(text.contains(words), "{dir}/LICENSE isn't the {license} license it's listed as");
    }
    let mut found = Vec::new();
    license_folders(root, root, &mut found);
    let listed = |dir: &PathBuf| VENDORED.iter().any(|(listed, _, _)| Path::new(listed) == dir);
    let missed: Vec<_> = VENDORED.iter().filter(|(dir, _, _)| !found.iter().any(|f| Path::new(dir) == f)).collect();
    assert!(missed.is_empty(), "the search for licenses didn't find these, so it isn't looking properly: {missed:?}");
    // The repo's own license, at its root, is the only one that isn't someone else's.
    let unlisted: Vec<_> = found.iter().filter(|dir| !dir.as_os_str().is_empty() && !listed(dir)).collect();
    assert!(unlisted.is_empty(), "someone else's files, so add them to `VENDORED`: {unlisted:?}");
}
