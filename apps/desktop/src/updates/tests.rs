use std::fmt::Write as _;
use std::path::Path;

use ring::digest::{SHA1_FOR_LEGACY_USE_ONLY, SHA256, digest};
use ring::signature::Ed25519KeyPair;
use velopack::locator::VelopackLocatorConfig;

use super::*;
use crate::updates::serving::{Reply, Server, serve};
use crate::updates::signing::{key_pair, public};

/// What `vpk` puts in an app it packages, as this test's app at 0.1.5.
const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package><metadata><id>Roundhouse</id><version>0.1.5</version><channel>osx</channel><mainExe>roundhouse-desktop</mainExe></metadata></package>"#;

/// An app Velopack installed at 0.1.5, in a directory of its own, and where it keeps downloaded packages.
fn installed() -> (tempfile::TempDir, VelopackLocatorConfig) {
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

fn package_name(version: &str) -> String {
    format!("Roundhouse-{version}-osx-full.nupkg")
}

/// Publishes a full package for each release, in a feed `signer` signs, and serves each package as its reply.
fn publish(releases: &[(&str, &[u8], Reply)], signer: &Ed25519KeyPair) -> Server {
    let assets: Vec<serde_json::Value> = releases
        .iter()
        .map(|(version, package, _)| {
            serde_json::json!({
                "PackageId": "Roundhouse",
                "Version": version,
                "Type": "Full",
                "FileName": package_name(version),
                "SHA1": hex(digest(&SHA1_FOR_LEGACY_USE_ONLY, package).as_ref()),
                "SHA256": hex(digest(&SHA256, package).as_ref()),
                "Size": package.len(),
            })
        })
        .collect();
    let feed = serde_json::to_vec(&serde_json::json!({ "Assets": assets })).unwrap();
    let signature = signer.sign(&feed).as_ref().to_vec();
    let mut paths = vec![
        ("releases.osx.json".to_string(), vec![Reply::file(feed)]),
        ("releases.osx.json.sig".to_string(), vec![Reply::file(signature)]),
    ];
    paths.extend(releases.iter().map(|(version, _, reply)| (package_name(version), vec![reply.clone()])));
    serve(paths)
}

/// The app's updater, set up as `start` sets it up, reading from `base`.
fn manager(base: &str, key: UpdateFeedKey, installed: &VelopackLocatorConfig) -> UpdateManager {
    let source = SignedSource::waiting(base, key, Duration::ZERO);
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
    // Bound and closed again, so nothing listens there.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();

    let problem = newest(&manager(&format!("http://127.0.0.1:{port}"), public(&key_pair()), &installed)).unwrap_err();

    assert_eq!(problem.kind, Kind::Unreachable);
}
