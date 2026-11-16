//! Only licenses we can ship do, in the crates and in the files kept from elsewhere.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::workspace::{metadata_for, workspace};

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

/// Someone else's files kept in this repo, as (the folder, its license's file, its SPDX license expression, words that
/// file has to hold). Not all of them are crates, so the license check of what ships can't see them all unless they're
/// listed here.
const VENDORED: &[(&str, &str, &str, &[&str])] = &[
    // Catppuccin's icons, manifest and colours, which `tools/file-icons/update.py` writes.
    ("apps/desktop/file-icons", "LICENSE", "MIT", &["Permission is hereby granted, free of charge"]),
    // gpui-kit and the crates under it, which `tools/kit-sync` writes. Each crate's manifest says so too.
    // `crates/component-macros` has a symbolic link to the copy's own `LICENSE-APACHE`, as upstream has it, which
    // the search for licenses skips, as it skips every link.
    ("third_party/gpui-kit/vendor", "LICENSE-APACHE", "Apache-2.0", APACHE),
    ("third_party/gpui-kit/vendor/crates/base", "LICENSE-APACHE", "Apache-2.0", APACHE),
    ("third_party/gpui-kit/vendor/crates/component", "LICENSE-APACHE", "Apache-2.0", APACHE),
    // The icons the kit draws, which are Lucide's, but those Lucide took from Feather, which are under MIT.
    ("third_party/gpui-kit/vendor/crates/assets", "LICENSE-LUCIDE", "ISC AND MIT", &["ISC License", "The MIT License"]),
];

/// What the kit's Apache license file holds: its notice, then the license's terms.
const APACHE: &[&str] =
    &["Licensed under the Apache License, Version 2.0", "TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION"];

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

/// Checks what ships, on the platforms it ships on, rather than everything in `Cargo.lock`, which also
/// holds build tools, proc macros and other platforms' crates. Add each platform here as it starts
/// shipping, such as iOS.
#[test]
fn only_allowed_licenses_ship() {
    const MAC: [&str; 2] = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
    const LINUX: [&str; 2] = ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"];
    const WINDOWS: [&str; 2] = ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"];
    let mut read = HashMap::new();
    for (root, targets) in [
        ("desktop", &[MAC[0], MAC[1], LINUX[0], LINUX[1], WINDOWS[0], WINDOWS[1]][..]),
        ("daemon", &[MAC[0], MAC[1], LINUX[0], LINUX[1], WINDOWS[0], WINDOWS[1]][..]),
        ("cli", &[MAC[0], MAC[1], LINUX[0], LINUX[1], WINDOWS[0], WINDOWS[1]][..]),
    ] {
        for target in targets {
            // Each platform's graph is the same for every root, so it's read once.
            let metadata = read.entry(*target).or_insert_with(|| metadata_for(target));
            let shipped = shipped_packages(metadata, root);
            assert!(shipped.len() > 1, "{root} on {target} should have dependencies");
            let bad: BTreeMap<_, _> =
                shipped.into_iter().filter(|(_, (license, source))| !may_ship(license, source.as_deref())).collect();
            assert!(bad.is_empty(), "{root} on {target} would ship licenses we can't: {bad:#?}");
        }
    }
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
    let metadata = workspace();
    let root = Path::new(metadata["workspace_root"].as_str().unwrap());
    for (dir, file, license, words) in VENDORED {
        assert!(may_ship(license, None), "{dir} is under {license}, which can't ship");
        let text = std::fs::read_to_string(root.join(dir).join(file)).unwrap_or_default();
        let missing: Vec<_> = words.iter().filter(|words| !text.contains(*words)).collect();
        assert!(missing.is_empty(), "{dir}/{file} isn't the {license} license it's listed as: no {missing:?}");
    }
    let mut found = Vec::new();
    license_folders(root, root, &mut found);
    let listed = |dir: &PathBuf| VENDORED.iter().any(|(listed, ..)| Path::new(listed) == dir);
    let missed: Vec<_> = VENDORED.iter().filter(|(dir, ..)| !found.iter().any(|f| Path::new(dir) == f)).collect();
    assert!(missed.is_empty(), "the search for licenses didn't find these, so it isn't looking properly: {missed:?}");
    // The repo's own license, at its root, is the only one that isn't someone else's.
    let unlisted: Vec<_> = found.iter().filter(|dir| !dir.as_os_str().is_empty() && !listed(dir)).collect();
    assert!(unlisted.is_empty(), "someone else's files, so add them to `VENDORED`: {unlisted:?}");
}
