use protocol::versions::Versions;

use super::*;

fn feed(json: &str) -> Feed {
    serde_json::from_str(json).unwrap()
}

fn full(version: &str, oldest: u32, newest: u32) -> String {
    format!(r#"{{"Version":"{version}","Type":"Full","WorkerVersions":{{"oldest":{oldest},"newest":{newest}}}}}"#)
}

/// A full package with the archive of its hub, as `publish.py` lists one on a channel that publishes them.
fn with_hub(version: &str, oldest: u32, newest: u32) -> String {
    let hub = format!(
        r#""Hub":{{"FileName":"Roundhouse-{version}-macos-arm64-hub.tar.gz","Size":9,"SHA1":"A","SHA256":"B"}}"#
    );
    format!(r#"{{"Version":"{version}","Type":"Full","WorkerVersions":{{"oldest":{oldest},"newest":{newest}}},{hub}}}"#)
}

fn assets(packages: &[String]) -> Feed {
    feed(&format!(r#"{{"Assets":[{}]}}"#, packages.join(",")))
}

/// `tools/package/test_publish.py` checks that the same file names the fields as `publish.py` writes them, so the two
/// can't come to call them different things, which would have every app that steps drop every release, and every hub
/// find no archive to update from.
#[test]
fn the_fixture_the_publisher_s_tests_check_says_what_its_hub_speaks_and_where_its_archive_is() {
    let feed: Feed = serde_json::from_slice(crate::fixture::FEED).unwrap();

    let releases: Vec<_> = feed.releases().collect();

    let archive = HubArchive {
        file_name: "Roundhouse-0.1.1-macos-arm64-hub.tar.gz".into(),
        size: 512,
        sha256: "89ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF01234567".into(),
    };
    let only =
        Release { version: "0.1.1", worker_versions: Some(Versions { oldest: 3, newest: 4 }), hub: Some(&archive) };
    assert_eq!(releases, [only]);
}

#[test]
fn a_release_is_its_full_package_and_speaks_what_that_says() {
    let feed = feed(&format!(
        r#"{{"Assets":[{},{{"Version":"0.1.7","Type":"Delta"}},{{"Version":"0.1.5","Type":"Full"}}]}}"#,
        full("0.1.7", 4, 5)
    ));

    let releases: Vec<_> = feed.releases().collect();

    assert_eq!(releases.iter().map(|release| release.version).collect::<Vec<_>>(), ["0.1.7", "0.1.5"]);
    assert!(releases[0].speaks(4) && releases[0].speaks(5) && !releases[0].speaks(3) && !releases[0].speaks(6));
    assert!(!releases[1].speaks(4), "from before releases said, so its hub may not");
}

/// A hub that can't talk to the newest release's workers strands the apps with those at work, and one that can strands
/// none, however old the releases before it.
#[test]
fn only_a_hub_past_the_newest_release_s_workers_strands_its_apps() {
    let feed = feed(&format!(r#"{{"Assets":[{},{}]}}"#, full("0.1.10", 4, 5), full("0.1.9", 3, 4)));

    let stranded = feed.stranded_by(Versions { oldest: 6, newest: 7 }).unwrap();

    assert_eq!(stranded.map(|release| release.version), Some("0.1.10"), "by version, not as text");
    assert_eq!(feed.stranded_by(Versions { oldest: 5, newest: 6 }).unwrap(), None);
    assert_eq!(feed.stranded_by(Versions { oldest: 4, newest: 5 }).unwrap(), None);
}

#[test]
fn a_feed_from_before_releases_said_or_with_none_strands_nothing() {
    let past = Versions { oldest: 9, newest: 10 };

    assert_eq!(feed(r#"{"Assets":[{"Version":"0.1.5","Type":"Full"}]}"#).stranded_by(past).unwrap(), None);
    assert_eq!(feed(r#"{"Assets":[{"Version":"0.1.5","Type":"Delta"}]}"#).stranded_by(past).unwrap(), None);
    assert_eq!(feed(r#"{"Assets":[]}"#).stranded_by(past).unwrap(), None);
}

#[test]
fn a_version_that_isnt_three_numbers_is_refused() {
    for version in ["0.1", "0.1.5.1", "0.1.x", "0.1.5-beta"] {
        let feed = feed(&format!(r#"{{"Assets":[{}]}}"#, full(version, 3, 4)));

        assert!(feed.stranded_by(Versions { oldest: 3, newest: 4 }).is_err(), "{version}");
    }
}

/// The newest that's newer, by version rather than as text, as the app's own update takes it.
#[test]
fn a_hub_updates_to_the_newest_release_with_an_archive() {
    let feed = assets(&[with_hub("0.1.9", 3, 4), with_hub("0.1.10", 3, 4), full("0.1.11", 3, 4)]);

    let update = feed.hub_update("0.1.8", 3).unwrap().unwrap();

    assert_eq!(update.version, "0.1.10", "the newest with an archive");
    assert_eq!(update.hub.unwrap().file_name, "Roundhouse-0.1.10-macos-arm64-hub.tar.gz");
}

#[test]
fn a_hub_that_s_the_newest_has_no_update() {
    let feed = assets(&[with_hub("0.1.9", 3, 4), with_hub("0.1.10", 3, 4)]);

    assert_eq!(feed.hub_update("0.1.10", 3).unwrap(), None);
    assert_eq!(feed.hub_update("0.1.11", 3).unwrap(), None, "newer than any in the feed");
}

/// So the update crosses one break in the worker protocol at a time, and ends none of the workers at work.
#[test]
fn a_hub_updates_only_to_a_release_that_speaks_its_oldest_worker() {
    let feed = assets(&[with_hub("0.1.9", 3, 4), with_hub("0.1.10", 4, 5), with_hub("0.1.11", 5, 6)]);

    assert_eq!(feed.hub_update("0.1.8", 3).unwrap().map(|release| release.version), Some("0.1.9"));
    assert_eq!(feed.hub_update("0.1.9", 4).unwrap().map(|release| release.version), Some("0.1.10"));
    assert_eq!(feed.hub_update("0.1.10", 4).unwrap(), None, "0.1.11 can't talk to workers of 4");
}

#[test]
fn a_hub_whose_version_isnt_a_release_s_is_refused() {
    let feed = assets(&[with_hub("0.1.9", 3, 4)]);

    assert!(feed.hub_update("4421741972d8", 3).is_err(), "a build of a commit, not a release");
    assert!(feed.hub_update("0.1.8-dirty-20261120234543", 3).is_err());
}

#[test]
fn an_archive_s_checksum_reads_as_its_bytes_in_either_case_and_anything_else_is_refused() {
    let archive = |sha256: &str| HubArchive { file_name: "hub.tar.gz".into(), size: 1, sha256: sha256.into() };
    let lower = "00ff".repeat(16);

    assert_eq!(archive(&lower).sha256_bytes().unwrap(), [0x00, 0xff].repeat(16).as_slice());
    assert_eq!(archive(&lower.to_uppercase()).sha256_bytes().unwrap(), archive(&lower).sha256_bytes().unwrap());
    for bad in ["", "00ff", &format!("{lower}00"), &"zz".repeat(32), &"é".repeat(32), &"+f".repeat(32)] {
        assert!(archive(bad).sha256_bytes().is_err(), "{bad}");
    }
}
