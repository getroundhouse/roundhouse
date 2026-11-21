use protocol::versions::Versions;

use crate::{Floors, Stranded};

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

/// A full package that says the oldest release it updates from.
fn updating_from(version: &str, oldest: u32, newest: u32, from: &str) -> String {
    full(version, oldest, newest).replace(r#","Type""#, &format!(r#","UpdatesFrom":"{from}","Type""#))
}

/// [`with_hub`], saying the oldest release it updates from.
fn with_hub_from(version: &str, oldest: u32, newest: u32, from: &str) -> String {
    with_hub(version, oldest, newest).replace(r#","Type""#, &format!(r#","UpdatesFrom":"{from}","Type""#))
}

/// The versions of the releases in `feed` an update from `running` can take.
fn taken(feed: &Feed, version: &str, workers: Option<u32>) -> Vec<String> {
    let running = Running { version, hub: HubRelease::On(version), workers };
    feed.taking_over(running).map(|release| release.version.to_owned()).collect()
}

fn assets(packages: &[String]) -> Feed {
    feed(&format!(r#"{{"Assets":[{}]}}"#, packages.join(",")))
}

/// `tools/package/test_publish.py` checks that the same file names the fields as `publish.py` writes them, so the two
/// can't come to call them different things, which would have every app that steps drop every release, every app step
/// past every floor, and every hub find no archive to update from.
#[test]
fn the_fixture_the_publisher_s_tests_check_says_its_floors_and_where_its_archive_is() {
    let feed: Feed = serde_json::from_slice(crate::fixture::FEED).unwrap();

    let releases: Vec<_> = feed.releases().collect();

    let archive = HubArchive {
        file_name: "Roundhouse-0.1.1-macos-arm64-hub.tar.gz".into(),
        size: 512,
        sha256: "89ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF01234567".into(),
    };
    let only = Release {
        version: "0.1.1",
        worker_versions: Some(Versions { oldest: 3, newest: 4 }),
        updates_from: Some("0.1.0"),
        hub: Some(&archive),
    };
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

/// A build's floors, speaking workers of `oldest` and the one after, and updating from `release` and nothing older.
fn floors(oldest: u32, release: Option<&'static str>) -> Floors {
    Floors { workers: Versions { oldest, newest: oldest + 1 }, release }
}

/// A hub that can't talk to the newest release's workers strands the apps with those at work, where apps keep them,
/// and one that can strands none, however old the releases before it.
#[test]
fn only_a_hub_past_the_newest_release_s_workers_strands_its_apps() {
    let feed = feed(&format!(r#"{{"Assets":[{},{}]}}"#, full("0.1.10", 4, 5), full("0.1.9", 3, 4)));

    let stranded = feed.stranded_by(floors(6, None), true).unwrap();

    let Some(Stranded::Workers(newest)) = stranded else { panic!("{stranded:?}") };
    assert_eq!(newest.version, "0.1.10", "by version, not as text");
    assert_eq!(feed.stranded_by(floors(5, None), true).unwrap(), None);
    assert_eq!(feed.stranded_by(floors(4, None), true).unwrap(), None);
    assert_eq!(feed.stranded_by(floors(6, None), false).unwrap(), None, "where apps keep no workers");
}

#[test]
fn a_feed_from_before_releases_said_or_with_none_strands_nothing() {
    let past = floors(9, None);

    assert_eq!(feed(r#"{"Assets":[{"Version":"0.1.5","Type":"Full"}]}"#).stranded_by(past, true).unwrap(), None);
    assert_eq!(feed(r#"{"Assets":[{"Version":"0.1.5","Type":"Delta"}]}"#).stranded_by(past, true).unwrap(), None);
    assert_eq!(feed(r#"{"Assets":[]}"#).stranded_by(past, true).unwrap(), None);
}

#[test]
fn a_version_that_isnt_three_numbers_is_refused() {
    for version in ["0.1", "0.1.5.1", "0.1.x", "0.1.5-beta"] {
        let feed = feed(&format!(r#"{{"Assets":[{}]}}"#, full(version, 3, 4)));

        assert!(feed.stranded_by(floors(3, None), true).is_err(), "{version}");
    }
}

/// Releases on either side of the break to protocol 5, and the one after: 0.1.9 is the last whose hub doesn't speak
/// workers of 5, and 0.1.11 the last that doesn't speak 6.
fn across_two_breaks() -> Feed {
    assets(&[full("0.1.8", 3, 4), full("0.1.9", 3, 4), full("0.1.10", 4, 5), full("0.1.11", 4, 5)])
}

/// An app on the newest release would have none to step to past a floor newer than it, and a person whose hub refuses
/// its database could install none that the feed doesn't keep.
#[test]
fn a_floor_strands_apps_unless_it_s_a_release_the_feed_keeps_to_step_to() {
    let feed = across_two_breaks();

    let newer = feed.stranded_by(floors(5, Some("0.1.12")), false).unwrap();
    assert!(matches!(newer, Some(Stranded::Release(newest)) if newest.version == "0.1.11"), "{newer:?}");
    assert_eq!(feed.stranded_by(floors(5, Some("0.1.8")), false).unwrap(), Some(Stranded::NotKept));
    assert_eq!(feed.stranded_by(floors(5, Some("0.1.11")), false).unwrap(), None);
    assert_eq!(feed.stranded_by(floors(5, Some("0.1.9")), false).unwrap(), None);
}

/// Only a release whose hub's workers are as new as the build's oldest takes it, so a floor no newer than the last
/// release before those carries even an app that doesn't read floors, where apps step.
#[test]
fn where_apps_step_a_floor_is_no_newer_than_the_last_release_before_the_build_s_oldest_workers() {
    let feed = across_two_breaks();

    assert_eq!(feed.stranded_by(floors(5, Some("0.1.9")), true).unwrap(), None);
    let past = feed.stranded_by(floors(5, Some("0.1.11")), true).unwrap();
    assert!(matches!(past, Some(Stranded::PastTheWorkerStep(Some(last))) if last.version == "0.1.9"), "{past:?}");
    let none_before = feed.stranded_by(floors(4, Some("0.1.9")), true).unwrap();
    assert_eq!(none_before, Some(Stranded::PastTheWorkerStep(None)), "no release in the feed is before workers of 4");
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

/// Its hub can't migrate the database of a release before it, by version rather than as text.
#[test]
fn a_release_is_taken_only_from_the_oldest_release_it_updates_from_or_a_newer_one() {
    let feed = assets(&[full("0.1.9", 3, 4), updating_from("0.1.11", 3, 4, "0.1.10")]);

    assert_eq!(taken(&feed, "0.1.9", None), ["0.1.9"]);
    assert_eq!(taken(&feed, "0.1.10", None), ["0.1.9", "0.1.11"]);
    assert_eq!(taken(&feed, "0.1.100", None), ["0.1.9", "0.1.11"]);
    assert_eq!(taken(&feed, "0.1.10-dirty-20261120234543", None), ["0.1.9"], "not known to be past it");
}

/// An update keeps its workers and its database at once, so it takes only what's past both floors.
#[test]
fn a_release_is_taken_only_past_both_its_floors() {
    let feed = assets(&[updating_from("0.1.11", 3, 4, "0.1.10"), updating_from("0.1.12", 4, 5, "0.1.9")]);

    assert_eq!(taken(&feed, "0.1.10", Some(3)), ["0.1.11"]);
    assert_eq!(taken(&feed, "0.1.10", Some(4)), ["0.1.11", "0.1.12"]);
    assert_eq!(taken(&feed, "0.1.9", Some(4)), ["0.1.12"]);
    assert!(taken(&feed, "0.1.9", Some(3)).is_empty());
}

/// The hub's database is from its own release, which can be older than the app's while the hub waits to restart, and
/// may be older than any floor while the app hasn't heard from the hub.
#[test]
fn a_release_is_taken_only_from_the_oldest_release_it_updates_from_for_the_hub_too() {
    let feed = assets(&[full("0.1.9", 3, 4), updating_from("0.1.11", 3, 4, "0.1.10")]);
    let taken = |hub| -> Vec<_> {
        feed.taking_over(Running { version: "0.1.10", hub, workers: None }).map(|release| release.version).collect()
    };

    assert_eq!(taken(HubRelease::On("0.1.9")), ["0.1.9"]);
    assert_eq!(taken(HubRelease::On("0.1.10")), ["0.1.9", "0.1.11"]);
    assert_eq!(taken(HubRelease::NotHeard), ["0.1.9"], "not heard from yet");
}

/// So an app drops the update it has ready when it hears of a hub on an older release than before, whose database that
/// update's hub may not migrate.
#[test]
fn only_a_hub_on_the_same_release_or_a_newer_one_is_past_every_floor_of_another() {
    let (on, not_heard) = (HubRelease::On, HubRelease::<&str>::NotHeard);

    assert!(on("0.1.10").past_every_floor_of(&on("0.1.9")), "by version, not as text");
    assert!(on("0.1.9").past_every_floor_of(&on("0.1.9")));
    assert!(!on("0.1.9").past_every_floor_of(&on("0.1.10")));
    assert!(on("0.1.9").past_every_floor_of(&not_heard));
    assert!(!not_heard.past_every_floor_of(&on("0.1.9")));
    assert!(!on("4421741972d8").past_every_floor_of(&on("0.1.9")), "a build of a commit is past none");
    assert!(on("0.1.9").past_every_floor_of(&on("4421741972d8")));
}

/// So a hub that's far behind steps through the release its update can't skip, as an app does.
#[test]
fn a_hub_updates_only_to_a_release_that_updates_from_it() {
    let feed = assets(&[with_hub("0.1.9", 3, 4), with_hub_from("0.1.10", 3, 4, "0.1.9")]);

    assert_eq!(feed.hub_update("0.1.8", 3).unwrap().map(|release| release.version), Some("0.1.9"));
    assert_eq!(feed.hub_update("0.1.9", 3).unwrap().map(|release| release.version), Some("0.1.10"));
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

/// `roundhoused worker-versions` prints a build's floors, and `tools/package/publish.py` reads them in this form, which
/// says nothing more than the worker versions until a build says the oldest release it updates from.
#[test]
fn a_build_s_floors_are_said_as_the_publisher_reads_them() {
    let say = |release| serde_json::to_string(&floors(3, release)).unwrap();

    assert_eq!(say(None), r#"{"oldest":3,"newest":4}"#);
    assert_eq!(say(Some("0.1.300")), r#"{"oldest":3,"newest":4,"updatesFrom":"0.1.300"}"#);
}
