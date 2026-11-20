//! What each crate may depend on, from `docs/principles/layers.md`, and the pins that have to agree: the tool that
//! packages the app and the crate that reads its packages, and the copy of gpui-kit, its tool's pin and the GPUI the
//! app draws with.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use kit_sync::{Git, Pin};
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
    for package in [
        "process",
        "protocol",
        "hub",
        "github",
        "worker",
        "agents",
        "providers",
        "client-core",
        "local-hub",
        "daemon",
        "cli",
    ] {
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
    for package in ["client-core", "local-hub", "cli", "mobile-ios"] {
        let closure = runtime_closure(&metadata, package);
        for banned in ["hub", "github", "worker", "agents", "providers"] {
            assert!(!closure.contains(banned), "{package} depends on {banned}");
        }
    }
}

/// Every client builds `client-core`, and a phone starts no process, so starting a hub on this machine, and what it
/// takes, are `local-hub`'s, which only the clients that start one build.
#[test]
fn client_core_and_the_phone_start_no_processes() {
    let metadata = metadata();
    for package in ["client-core", "mobile-ios"] {
        let closure = runtime_closure(&metadata, package);
        for banned in ["local-hub", "process", "private-files"] {
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

/// The app builds gpui-kit's crates from its copy, so a change to the copy is what the app gets, and no dependency
/// brings another kit, whose types wouldn't be the copy's.
#[test]
fn the_app_builds_the_kit_from_its_copy() {
    let metadata = metadata();
    let copy = Path::new(metadata["workspace_root"].as_str().unwrap()).join("third_party/gpui-kit/vendor");
    let packages = metadata["packages"].as_array().unwrap();
    let in_copy = |package: &Value| {
        Path::new(package["manifest_path"].as_str().unwrap()).starts_with(&copy) && package["source"].is_null()
    };
    let from_copy: BTreeSet<&str> =
        packages.iter().filter(|package| in_copy(package)).map(|package| package["name"].as_str().unwrap()).collect();
    assert!(from_copy.contains("gpui-kit"), "the app builds the kit from the copy, but only these: {from_copy:?}");
    let elsewhere: Vec<&str> = (packages.iter())
        .filter(|package| from_copy.contains(package["name"].as_str().unwrap()) && !in_copy(package))
        .map(|package| package["id"].as_str().unwrap())
        .collect();
    assert!(elsewhere.is_empty(), "these aren't the copy's, so the app builds a kit of their own: {elsewhere:?}");
}

/// `vpk` makes the packages and the feed that the app's `velopack` reads, and Velopack releases the two together,
/// so they stay on one version. `tools/package/dotnet-tools.json` pins `vpk`.
#[test]
fn vpk_is_the_velopack_crate_s_version() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let tools = root.join("tools/package/dotnet-tools.json");
    let tools: Value = serde_json::from_str(&std::fs::read_to_string(tools).unwrap()).unwrap();

    assert_eq!(locked("velopack"), [tools["tools"]["vpk"]["version"].as_str().unwrap()]);
}

/// The versions of `name` that `Cargo.lock` has, which are the versions every build takes.
fn locked(name: &str) -> Vec<String> {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let lock: toml::Table = std::fs::read_to_string(root.join("Cargo.lock")).unwrap().parse().unwrap();
    (lock["package"].as_array().unwrap().iter())
        .filter(|package| package["name"].as_str() == Some(name))
        .map(|package| package["version"].as_str().unwrap().to_string())
        .collect()
}

/// The GPUI the app draws with off the main thread is the GPUI the kit is built on, since each would have types of its
/// own. `tools/kit-sync` moves both.
#[test]
fn gpui_pre_apple_is_the_kit_s_gpui() {
    let gpui = locked("gpui-pre");
    assert_eq!(gpui.len(), 1, "the app builds one GPUI: {gpui:?}");
    assert_eq!(locked("gpui-pre-apple"), gpui, "run `cargo run -p kit-sync`, which moves gpui-pre-apple");
}

/// Each crate copied whole into `third_party/`, as that folder's README says, is the one every build takes, rather than
/// the release on crates.io. A copy whose version no longer matches what's asked for, such as GPUI's after
/// `tools/kit-sync` moves it, only has Cargo warn that the patch went unused, and the build takes the release without
/// our changes. GPUI's copies are on the GPUI the app builds, since each would have types of its own.
#[test]
fn every_copied_crate_is_the_one_the_app_builds() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let lock: toml::Table = std::fs::read_to_string(root.join("Cargo.lock")).unwrap().parse().unwrap();
    let gpui = locked("gpui-pre");
    for (name, folder, on_gpui) in [
        ("gpui-pre-apple", "gpui-pre-apple", true),
        ("gpui-pre-ios", "gpui-pre-ios", true),
        ("zed-font-kit", "zed-font-kit", false),
    ] {
        let manifest: toml::Table =
            std::fs::read_to_string(root.join("third_party").join(folder).join("Cargo.toml")).unwrap().parse().unwrap();
        let copy = manifest["package"]["version"].as_str().unwrap();
        let built: Vec<_> = (lock["package"].as_array().unwrap().iter())
            .filter(|package| package["name"].as_str() == Some(name))
            .map(|package| (package["version"].as_str().unwrap(), package.get("source").is_none()))
            .collect();
        assert_eq!(built, [(copy, true)], "the app builds {name} from crates.io, not the copy in third_party/{folder}");
        if on_gpui {
            assert_eq!(gpui, [copy], "third_party/{folder} isn't on the GPUI the app builds: take its release again");
        }
    }
}

/// The copy of gpui-kit is on the release `tools/kit-sync` pins, with only the patches in `third_party/gpui-kit/patches`
/// on top, notices on the files they change, and no conflict left to settle, so neither an unfinished sync nor an
/// unrecorded change to the copy can merge.
#[test]
fn the_kit_s_copy_is_on_the_release_its_tool_pins() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let git = Git::new(std::env::vars_os());

    if let Err(error) = kit_sync::verify(&git, root, &Pin::ours()) {
        panic!("{error}");
    }
}
