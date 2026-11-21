use protocol::versions::Versions;

use super::*;

fn feed(json: &str) -> Feed {
    serde_json::from_str(json).unwrap()
}

fn full(version: &str, oldest: u32, newest: u32) -> String {
    format!(r#"{{"Version":"{version}","Type":"Full","WorkerVersions":{{"oldest":{oldest},"newest":{newest}}}}}"#)
}

/// `tools/package/test_publish.py` checks that the same file names the field as `publish.py` writes it, so the two
/// can't come to call it different things, which would have every app that steps drop every release.
#[test]
fn the_fixture_the_publisher_s_tests_check_says_what_its_hub_speaks() {
    let fixture = include_bytes!("../fixture/releases.osx.json");
    let feed: Feed = serde_json::from_slice(fixture).unwrap();

    let releases: Vec<_> = feed.releases().collect();

    assert_eq!(releases, [Release { version: "0.1.1", worker_versions: Some(Versions { oldest: 3, newest: 4 }) }]);
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
