use super::*;

/// Long enough for a look at a server on this Mac.
pub(super) const LOOK_TIMEOUT: Duration = Duration::from_secs(10);

/// What `vpk` puts in an app it packages, as this test's app at 0.1.5.
const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package><metadata><id>Roundhouse</id><version>0.1.5</version><channel>macos-arm64</channel><mainExe>roundhouse-desktop</mainExe></metadata></package>"#;

/// An app Velopack installed at 0.1.5, in a directory of its own, and where it keeps downloaded packages.
pub(super) fn installed() -> (tempfile::TempDir, VelopackLocatorConfig) {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("Roundhouse.app");
    let binaries = bundle.join("Contents/MacOS");
    std::fs::create_dir_all(&binaries).unwrap();
    std::fs::write(binaries.join("UpdateMac"), "").unwrap();
    std::fs::write(binaries.join("sq.version"), MANIFEST).unwrap();
    let config = VelopackLocatorConfig {
        RootAppDir: bundle,
        UpdateExePath: binaries.join("UpdateMac"),
        PackagesDir: dir.path().join("packages"),
        ManifestPath: binaries.join("sq.version"),
        CurrentBinaryDir: binaries,
        IsPortable: true,
    };
    (dir, config)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        write!(text, "{byte:02X}").unwrap();
        text
    })
}

pub(super) fn package_name(version: &str) -> String {
    format!("Roundhouse-{version}-macos-arm64-full.nupkg")
}

/// Publishes a full package for each release, in a feed `signer` signs, and serves each package as its reply.
pub(super) fn publish(releases: &[(&str, &[u8], Reply)], signer: &Ed25519KeyPair) -> Server {
    let speaking: Vec<_> =
        releases.iter().map(|(version, package, reply)| (*version, *package, reply.clone(), None)).collect();
    publish_speaking(&speaking, signer)
}

/// The same, with the worker versions each release's hub speaks, if the feed says.
pub(super) fn publish_speaking(releases: &[(&str, &[u8], Reply, Option<Versions>)], signer: &Ed25519KeyPair) -> Server {
    let assets: Vec<serde_json::Value> = releases
        .iter()
        .map(|(version, package, _, speaks)| {
            let mut asset = serde_json::json!({
                "PackageId": "Roundhouse",
                "Version": version,
                "Type": "Full",
                "FileName": package_name(version),
                "SHA1": hex(digest(&SHA1_FOR_LEGACY_USE_ONLY, package).as_ref()),
                "SHA256": hex(digest(&SHA256, package).as_ref()),
                "Size": package.len(),
            });
            if let Some(speaks) = speaks {
                asset["WorkerVersions"] = serde_json::to_value(speaks).unwrap();
            }
            asset
        })
        .collect();
    let feed = serde_json::to_vec(&serde_json::json!({ "Assets": assets })).unwrap();
    let signature = signer.sign(&feed).as_ref().to_vec();
    let mut paths = vec![
        ("releases.macos-arm64.json".to_string(), vec![Reply::file(feed)]),
        ("releases.macos-arm64.json.sig".to_string(), vec![Reply::file(signature)]),
    ];
    paths.extend(releases.iter().map(|(version, _, reply, _)| (package_name(version), vec![reply.clone()])));
    serve(paths).unwrap()
}

/// The app's updater, set up as `start` sets it up, reading from `base`, keeping no worker.
fn manager(base: &str, key: UpdateFeedKey, installed: &VelopackLocatorConfig) -> UpdateManager {
    manager_keeping(base, key, installed, &ToKeep::oldest_at(None))
}

pub(super) fn manager_keeping(
    base: &str,
    key: UpdateFeedKey,
    installed: &VelopackLocatorConfig,
    workers: &ToKeep,
) -> UpdateManager {
    let source = SignedSource::waiting(base, key, workers.clone(), Duration::ZERO);
    UpdateManager::new(source, Some(options()), Some(installed.clone())).unwrap()
}

fn downloaded(installed: &VelopackLocatorConfig) -> Vec<String> {
    let found = std::fs::read_dir(&installed.PackagesDir).into_iter().flatten();
    let mut names: Vec<String> = found.map(|entry| entry.unwrap().file_name().to_string_lossy().to_string()).collect();
    names.retain(|name| Path::new(name).extension().is_some_and(|extension| extension == "nupkg"));
    names
}

#[test]
fn a_newer_release_in_a_signed_feed_downloads_and_passes_its_checks() {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let new = b"0.1.6, packaged".as_slice();
    let server = publish(&[("0.1.5", b"", Reply::file("")), ("0.1.6", new, Reply::file(new))], &pair);

    let found = newest(&manager(&server.base, public(&pair), &installed)).unwrap().unwrap();

    assert_eq!(found.TargetFullRelease.Version, "0.1.6");
    assert_eq!(std::fs::read(installed.PackagesDir.join(package_name("0.1.6"))).unwrap(), new);
}

#[test]
fn a_feed_signed_with_another_key_is_refused_and_nothing_is_downloaded() {
    let (_dir, installed) = installed();
    let new = b"0.1.6, packaged".as_slice();
    let server = publish(&[("0.1.6", new, Reply::file(new))], &key_pair());

    let problem = newest(&manager(&server.base, public(&key_pair()), &installed)).unwrap_err();

    assert_eq!(problem.kind, Kind::Refused(Refusal::Unsigned));
    assert!(server.asked().iter().all(|path| !path.contains(".nupkg")), "{:?}", server.asked());
    assert!(downloaded(&installed).is_empty());
}

#[test]
fn a_feed_of_older_releases_offers_nothing() {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let server = publish(&[("0.1.4", b"0.1.4, packaged", Reply::file("0.1.4, packaged"))], &pair);

    assert!(newest(&manager(&server.base, public(&pair), &installed)).unwrap().is_none());
    assert!(downloaded(&installed).is_empty());
}

#[test]
fn a_package_changed_from_the_one_the_feed_names_is_refused() {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let server = publish(&[("0.1.6", b"0.1.6, packaged", Reply::file("0.1.6, changed!"))], &pair);

    let problem = newest(&manager(&server.base, public(&pair), &installed)).unwrap_err();

    assert_eq!(problem.kind, Kind::Refused(Refusal::NotAsListed));
    assert!(downloaded(&installed).is_empty());
}

#[test]
fn a_package_longer_than_the_feed_says_is_refused() {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let server = publish(&[("0.1.6", b"0.1.6, packaged", Reply::file("0.1.6, packaged, and then some"))], &pair);

    let problem = newest(&manager(&server.base, public(&pair), &installed)).unwrap_err();

    assert_eq!(problem.kind, Kind::Refused(Refusal::TooLong));
    assert!(downloaded(&installed).is_empty());
}

#[test]
fn a_server_that_isnt_there_is_out_of_reach() {
    let (_dir, installed) = installed();

    let problem = newest(&manager(NOWHERE, public(&key_pair()), &installed)).unwrap_err();

    assert_eq!(problem.kind, Kind::Unreachable);
}

/// How many times the server was asked for the update feed, which is once a look.
pub(super) fn feeds_read(server: &Server) -> usize {
    server.asked().iter().filter(|path| *path == "releases.macos-arm64.json").count()
}

#[test]
fn asking_looks_again_without_waiting_for_the_hour() {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let server = publish(&[("0.1.5", b"", Reply::file(""))], &pair);
    let manager = manager(&server.base, public(&pair), &installed);
    let state = tempfile::tempdir().unwrap();
    let (look_now, asked) = flume::bounded(1);
    let (looks, looked) = flume::unbounded();
    let workers = ToKeep::oldest_at(None);
    std::thread::spawn(move || keep_looking(&manager, &workers, &asked, &looks, &OutOfReach::in_dir(state.path())));
    let (on_launch, _) = looked.recv_timeout(LOOK_TIMEOUT).unwrap();
    assert!(on_launch.unwrap().is_none());

    look_now.send(()).unwrap();

    let (asked_for, _) = looked.recv_timeout(LOOK_TIMEOUT).unwrap();
    assert!(asked_for.unwrap().is_none());
    assert_eq!(feeds_read(&server), 2);
}
