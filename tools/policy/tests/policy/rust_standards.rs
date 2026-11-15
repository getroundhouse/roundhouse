//! The Rust standards every crate keeps: how long a file can be, and the workspace's lints.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::workspace::workspace;

/// The most lines a Rust file can have, tests included. `docs/rust.md` has the reasoning.
const MAX_FILE_LINES: usize = 400;

/// Files allowed past that limit, as (path from the repo root, why). Empty so far.
const LONG_FILES: &[(&str, &str)] = &[];

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
    let metadata = workspace();
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
    let metadata = workspace();
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
