//! What each crate may depend on, from `docs/principles/layers.md`, and the pins that have to agree: the tool that
//! packages the app and the crate that reads its packages, the copy of gpui-kit, its tool's pin and the GPUI the app
//! draws with, and what's kept of libvpx and its script's pin.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use ring::digest::{SHA256, digest};

use kit_sync::{Git, Pin};
use serde_json::Value;

use super::workspace::{built_by, metadata};

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
    let closure = runtime_closure(metadata, "protocol");
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
        let gpui: Vec<_> = runtime_closure(metadata, package)
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
        let closure = runtime_closure(metadata, package);
        for banned in ["hub", "github", "worker", "agents", "providers"] {
            assert!(!closure.contains(banned), "{package} depends on {banned}");
        }
    }
}

/// No client builds Ghostty. The worker keeps the only copy of each terminal, and a client shows the rows it sends.
/// Tests still may, through `client-core`'s `testing` feature, for the fake hub to make pictures from its world's
/// bytes. `cargo metadata` resolves features for the whole workspace at once, tests and all, so it counts that as
/// what ships, and this asks `cargo tree`, which resolves them as a build of each client does.
#[test]
fn no_client_builds_ghostty() {
    let clients =
        ["client-core", "terminal-view", "transcript", "client-ui", "local-hub", "cli", "desktop", "mobile-ios"];
    // Asked for at once, since Cargo does part of each outside its lock, as the licenses' graphs are.
    let builds: Vec<Vec<String>> = std::thread::scope(|scope| {
        let building: Vec<_> = clients.map(|package| scope.spawn(move || built_by(package))).into();
        building.into_iter().map(|built| built.join().unwrap()).collect()
    });
    for (package, built) in clients.into_iter().zip(builds) {
        assert!(built.contains(&"client-core".to_string()), "{package} builds client-core, as every client does");
        for banned in ["terminal-state", "libghostty-vt"] {
            assert!(!built.contains(&banned.to_string()), "{package} builds {banned}");
        }
    }
}

/// Every client builds `client-core`, and a phone starts no process, so starting a hub on this machine, and what it
/// takes, are `local-hub`'s, which only the clients that start one build.
#[test]
fn client_core_and_the_phone_start_no_processes() {
    let metadata = metadata();
    for package in ["client-core", "mobile-ios"] {
        let closure = runtime_closure(metadata, package);
        for banned in ["local-hub", "process", "private-files"] {
            assert!(!closure.contains(banned), "{package} depends on {banned}");
        }
    }
}

/// Checks that `package` needs none of our other crates but those in `but`, no async runtime and no UI, so any client
/// on any device can use it.
fn needs_none_of_ours_no_async_runtime_and_no_ui(package: &str, but: &[&str]) {
    let metadata = metadata();
    let closure = runtime_closure(metadata, package);
    let packages = metadata["packages"].as_array().unwrap();
    let ours: Vec<&str> =
        packages.iter().filter(|p| p["source"].is_null()).filter_map(|p| p["name"].as_str()).collect();
    assert!(ours.contains(&"protocol"), "ours are the packages with no source, which come from paths in the workspace");
    for banned in ours.into_iter().filter(|name| !but.contains(name)).chain(["tokio", "async-std", "smol", "gpui"]) {
        assert!(!closure.contains(banned), "{package} depends on {banned}");
    }
}

/// Dictation is the device's alone, with threads rather than a runtime and no UI, so any client on any device can hear
/// with it. It keeps its model's files with `models`, which downloads them with `fetching`, and hears the microphone
/// and quiets what's playing with `sound`, which pauses it with `now-playing`, which are free of the same.
#[test]
fn dictation_needs_none_of_our_other_crates_but_models_and_sound_no_async_runtime_and_no_ui() {
    needs_none_of_ours_no_async_runtime_and_no_ui("dictation", &["models", "fetching", "sound", "now-playing"]);
}

/// The device's sound knows nothing of speech, with threads rather than a runtime and no UI, so any client on any
/// device can hear or play with it. It pauses what's playing with `now-playing`, which is free of the same.
#[test]
fn sound_needs_none_of_our_other_crates_but_now_playing_no_async_runtime_and_no_ui() {
    needs_none_of_ours_no_async_runtime_and_no_ui("sound", &["now-playing"]);
}

/// Reading out knows nothing of chats or the hub, with threads rather than a runtime and no UI, so any client on any
/// device can read with it. It speaks with `phonon` and plays through `sound`, which are free of the same.
#[test]
fn speaking_needs_none_of_our_other_crates_but_phonon_and_sound_no_async_runtime_and_no_ui() {
    needs_none_of_ours_no_async_runtime_and_no_ui(
        "speaking",
        &["phonon", "sound", "models", "fetching", "now-playing"],
    );
}

/// Conversation mode's voice is every client's, with threads rather than a runtime and no UI, so the phone can read
/// out with it as the desktop app does. It joins `client-core`'s reading out to `speaking`'s voice, so `client-core`
/// itself stays free of audio.
#[test]
fn conversation_needs_none_of_our_other_crates_but_client_core_and_speaking_no_async_runtime_and_no_ui() {
    needs_none_of_ours_no_async_runtime_and_no_ui(
        "conversation",
        &[
            "client-core",
            "protocol",
            "terminal-state",
            "speaking",
            "phonon",
            "sound",
            "models",
            "fetching",
            "now-playing",
        ],
    );
}

/// `client-core` reads replies out without a voice of its own, so the CLI never builds one.
#[test]
fn client_core_has_no_voice() {
    let closure = runtime_closure(metadata(), "client-core");
    for banned in ["speaking", "phonon", "sound", "dictation"] {
        assert!(!closure.contains(banned), "client-core depends on {banned}");
    }
}

/// Phonon knows nothing of what it says or where the audio goes, with no runtime and no UI, so any client on any device
/// can speak with it. It keeps its model's files with `models`, which downloads them with `fetching`.
#[test]
fn phonon_needs_none_of_our_other_crates_but_models_no_async_runtime_and_no_ui() {
    needs_none_of_ours_no_async_runtime_and_no_ui("phonon", &["models", "fetching"]);
}

/// A model's files know nothing of what the model does, and are downloaded on the caller's thread, so any client on any
/// device can keep a model with them.
#[test]
fn models_need_none_of_our_other_crates_but_fetching_no_async_runtime_and_no_ui() {
    needs_none_of_ours_no_async_runtime_and_no_ui("models", &["fetching"]);
}

/// Fetching knows nothing of what it fetches, and runs on the caller's thread, so any client on any device can fetch
/// with it.
#[test]
fn fetching_needs_none_of_our_other_crates_no_async_runtime_and_no_ui() {
    needs_none_of_ours_no_async_runtime_and_no_ui("fetching", &[]);
}

/// `now-playing` knows nothing of what's dictated, and runs on the caller's thread, so any client on any device can
/// pause with it.
#[test]
fn now_playing_needs_none_of_our_other_crates_no_async_runtime_and_no_ui() {
    needs_none_of_ours_no_async_runtime_and_no_ui("now-playing", &[]);
}

/// Decoding video knows nothing of where a video is shown, with threads rather than a runtime and no UI, so any client
/// on any device can play with it. What its build script compiles is `libvpx-build`'s, which only builds it.
#[test]
fn video_needs_none_of_our_other_crates_no_async_runtime_and_no_ui() {
    needs_none_of_ours_no_async_runtime_and_no_ui("video", &[]);
}

#[test]
fn the_hub_and_the_worker_stay_apart() {
    let metadata = metadata();
    assert!(!runtime_closure(metadata, "hub").contains("worker"));
    assert!(!runtime_closure(metadata, "hub").contains("agents"));
    assert!(!runtime_closure(metadata, "worker").contains("hub"));
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

/// `third_party/libvpx` is for the release and commit `tools/libvpx-sync.sh` pins, file for file as the script wrote
/// it, by the SHA-256 it wrote of each, so neither a release half taken nor a change the script didn't make can merge.
/// The `video` crate's build script fetches the source at the commit it names.
#[test]
fn libvpx_s_folder_is_what_its_script_wrote_for_the_release_it_pins() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let folder = root.join("third_party/libvpx");
    let script = std::fs::read_to_string(root.join("tools/libvpx-sync.sh")).unwrap();
    let pin = |name: &str| script.lines().find_map(|line| line.strip_prefix(&format!("{name}="))).unwrap().to_string();
    let version = std::fs::read_to_string(folder.join("config/vpx_version.h")).unwrap();
    let commit = std::fs::read_to_string(folder.join("COMMIT")).unwrap();
    let sums = std::fs::read_to_string(folder.join("SHA256SUMS")).unwrap();

    let listed: BTreeMap<String, String> = sums
        .lines()
        .map(|line| line.split_once("  ").unwrap())
        .map(|(sum, path)| (path.to_string(), sum.to_string()))
        .collect();
    let mut found = BTreeMap::new();
    sums_under(&folder, &folder, &mut found);
    found.remove("SHA256SUMS");
    let (release, pinned) = (pin("LIBVPX_VERSION"), pin("LIBVPX_COMMIT"));
    let named = format!("#define VERSION_STRING_NOSP \"{release}\"");
    assert!(version.contains(&named), "third_party/libvpx isn't {release}, which tools/libvpx-sync.sh pins. Run it");
    assert_eq!(commit.trim(), pinned, "third_party/libvpx isn't the commit tools/libvpx-sync.sh pins. Run it");
    assert!(
        found == listed,
        "third_party/libvpx isn't what tools/libvpx-sync.sh wrote. It changes only by running the script, as \
         third_party/README.md says"
    );
}

/// The SHA-256 of each file under `dir`, by its path from `folder`, as `sha256sum` writes them.
fn sums_under(folder: &Path, dir: &Path, found: &mut BTreeMap<String, String>) {
    for entry in std::fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            sums_under(folder, &path, found);
        } else {
            let sum = digest(&SHA256, &std::fs::read(&path).unwrap());
            let hex = sum.as_ref().iter().fold(String::new(), |mut hex, byte| {
                write!(hex, "{byte:02x}").unwrap();
                hex
            });
            let relative = path.strip_prefix(folder).unwrap().to_string_lossy().replace('\\', "/");
            found.insert(relative, hex);
        }
    }
}
