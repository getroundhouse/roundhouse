use update_feed::{Feed, HubRelease, Running, UpdateFeedKey, fixture};

use super::*;
use crate::updates::signing::{key_pair, public};
use fetching::serving::{Reply, Server, serve};

/// The shape `vpk` writes, with one release.
const FEED: &str = r#"{"Assets":[{"PackageId":"Roundhouse","Version":"0.1.50","Type":"Full","FileName":"Roundhouse-0.1.50-macos-arm64-full.nupkg","SHA1":"AD4CF1813CB4589407AEEB7DA69063A25FF84566","SHA256":"0F568A995C5F5474207B3A3225F7B4059D2A268C35ED32310F1083E59CA286B7","Size":18889777}]}"#;

/// A feed, its signature, and the key that checks it, as the release workflow makes them.
fn signed(feed: &str) -> (Vec<u8>, Vec<u8>, UpdateFeedKey) {
    let pair = key_pair();
    (feed.as_bytes().to_vec(), pair.sign(feed.as_bytes()).as_ref().to_vec(), public(&pair))
}

/// Serves a feed and its signature, each as a list of replies in turn.
fn serve_feed(feed: Vec<Reply>, signature: Vec<Reply>) -> Server {
    serve(vec![
        ("releases.macos-arm64.json".to_string(), feed),
        ("releases.macos-arm64.json.sig".to_string(), signature),
    ])
    .unwrap()
}

/// Reads the feed through Velopack's interface, without waiting to read it again.
fn read_feed(server: &Server, key: UpdateFeedKey) -> Result<VelopackAssetFeed, velopack::Error> {
    let source = SignedSource::waiting(&server.base, key, ToKeep::oldest_at(None), Duration::ZERO);
    source.get_release_feed("macos-arm64", &Manifest::default(), "")
}

/// The source's own error, which Velopack passes on inside its `Io` variant.
fn source_error<T: std::fmt::Debug>(result: Result<T, velopack::Error>) -> SourceError {
    match result {
        Err(velopack::Error::Io(io)) => *io.into_inner().unwrap().downcast::<SourceError>().unwrap(),
        other => panic!("not the source's error: {other:?}"),
    }
}

fn asked_for_the_feed(server: &Server) -> usize {
    server.asked().iter().filter(|path| *path == "releases.macos-arm64.json").count()
}

/// Velopack reads the feed as the release workflow publishes it, whatever it says beyond what Velopack knows, such as
/// which worker versions a release's hub speaks and where its hub's archive is.
#[test]
fn a_feed_openssl_signed_is_read() {
    let releases = verified(fixture::FEED, fixture::SIGNATURE, &fixture::KEY).unwrap();

    assert_eq!(releases.Assets[0].Version, "0.1.1");
}

#[test]
fn a_feed_signed_with_the_key_is_read() {
    let (feed, signature, key) = signed(FEED);

    let releases = verified(&feed, &signature, &key).unwrap();

    assert_eq!(releases.Assets.len(), 1);
    assert_eq!(releases.Assets[0].Version, "0.1.50");
}

#[test]
fn nothing_is_read_from_a_feed_before_its_signature_checks_out() {
    let (feed, signature, key) = signed("not a feed");

    assert!(matches!(verified(b"not a feed either", &signature, &key), Err(SourceError::Unsigned(_))));
    // Signed, so it's read, and only then found not to be a feed.
    assert!(matches!(verified(&feed, &signature, &key), Err(SourceError::Unreadable(_))));
}

#[test]
fn the_source_reads_a_feed_signed_with_the_key() {
    let (feed, signature, key) = signed(FEED);
    let server = serve_feed(vec![Reply::file(feed)], vec![Reply::file(signature)]);

    assert_eq!(read_feed(&server, key).unwrap().Assets.len(), 1);
    assert_eq!(server.asked(), ["releases.macos-arm64.json", "releases.macos-arm64.json.sig"]);
}

#[test]
fn the_source_refuses_a_feed_signed_with_another_key_after_reading_it_again() {
    let (feed, signature, _) = signed(FEED);
    let server = serve_feed(vec![Reply::file(feed)], vec![Reply::file(signature)]);

    assert!(matches!(source_error(read_feed(&server, public(&key_pair()))), SourceError::Unsigned(_)));
    assert_eq!(asked_for_the_feed(&server), 2);
}

#[test]
fn the_source_refuses_a_feed_changed_after_it_was_signed() {
    let (feed, signature, key) = signed(FEED);
    let changed = String::from_utf8(feed).unwrap().replace("0.1.50", "9.1.50");
    let server = serve_feed(vec![Reply::file(changed)], vec![Reply::file(signature)]);

    assert!(matches!(source_error(read_feed(&server, key)), SourceError::Unsigned(_)));
}

/// The release workflow uploads the feed, then its signature, so a check can come between them.
#[test]
fn the_source_reads_again_a_feed_whose_signature_was_still_on_its_way() {
    let pair = key_pair();
    let new_feed = FEED.replace("0.1.50", "0.1.51");
    let old_signature = pair.sign(FEED.as_bytes()).as_ref().to_vec();
    let new_signature = pair.sign(new_feed.as_bytes()).as_ref().to_vec();
    let server = serve_feed(vec![Reply::file(new_feed)], vec![Reply::file(old_signature), Reply::file(new_signature)]);

    assert_eq!(read_feed(&server, public(&pair)).unwrap().Assets[0].Version, "0.1.51");
    assert_eq!(asked_for_the_feed(&server), 2);
}

#[test]
fn a_signature_is_checked_on_the_feed_as_decompressed() {
    let (feed, signature, key) = signed(FEED);
    let server = serve_feed(vec![Reply::gzipped(&feed).unwrap()], vec![Reply::file(signature)]);

    assert_eq!(read_feed(&server, key).unwrap().Assets.len(), 1);
}

#[test]
fn a_feed_longer_than_the_limit_once_decompressed_is_refused() {
    let (_, signature, key) = signed(FEED);
    let long = vec![b' '; usize::try_from(FEED_LIMIT_BYTES).unwrap() + 1];
    let server = serve_feed(vec![Reply::gzipped(&long).unwrap()], vec![Reply::file(signature)]);

    assert!(matches!(source_error(read_feed(&server, key)), SourceError::Fetch(fetching::FetchError::TooLong { .. })));
}

#[test]
fn a_redirect_is_not_followed() {
    let (feed, signature, key) = signed(FEED);
    let elsewhere = serve_feed(vec![Reply::file(feed)], vec![Reply::file(signature)]);
    let server = serve_feed(vec![Reply::redirect(&format!("{}/releases.macos-arm64.json", elsewhere.base))], vec![]);

    assert!(matches!(
        source_error(read_feed(&server, key)),
        SourceError::Fetch(fetching::FetchError::Status { status: 302, .. })
    ));
    assert!(elsewhere.asked().is_empty());
}

/// A package the feed says is `size` bytes, served as `reply`, downloaded to a file of its own.
fn download(size: u64, reply: Reply) -> (Result<(), velopack::Error>, tempfile::TempDir) {
    let name = "Roundhouse-0.1.50-macos-arm64-full.nupkg";
    let server = serve(vec![(name.to_string(), vec![reply])]).unwrap();
    let asset = VelopackAsset { FileName: name.to_string(), Size: size, ..VelopackAsset::default() };
    let dir = tempfile::tempdir().unwrap();
    let source = SignedSource::new(&server.base, public(&key_pair()), ToKeep::oldest_at(None));
    (source.download_release_entry(&asset, &dir.path().join("package.partial"), None), dir)
}

#[test]
fn a_package_downloads_as_long_as_the_feed_says() {
    let (downloaded, dir) = download(5, Reply::gzipped(b"12345").unwrap());

    downloaded.unwrap();
    assert_eq!(std::fs::read(dir.path().join("package.partial")).unwrap(), b"12345");
}

#[test]
fn a_package_longer_than_the_feed_says_once_decompressed_is_refused_and_not_kept() {
    let (downloaded, dir) = download(5, Reply::gzipped(&vec![b'0'; 10 * 1024 * 1024]).unwrap());

    assert!(matches!(
        source_error(downloaded),
        SourceError::Fetch(fetching::FetchError::TooLong { limit_bytes: 5, .. })
    ));
    assert!(!dir.path().join("package.partial").exists());
}

#[test]
fn a_package_cut_short_is_not_kept() {
    let (downloaded, dir) = download(6, Reply::file("12345"));

    assert!(matches!(
        source_error(downloaded),
        SourceError::Fetch(fetching::FetchError::CutShort { expected_bytes: 6, got_bytes: 5, .. })
    ));
    assert!(!dir.path().join("package.partial").exists());
}

/// Releases whose hubs speak worker protocols 3 to 4, and 4 to 5, as `tools/package/publish.py` writes them, after one
/// from before releases said.
const SPEAKING: &str = r#"{"Assets":[
    {"PackageId":"Roundhouse","Version":"0.1.5","Type":"Full","FileName":"a","SHA1":"","Size":1},
    {"PackageId":"Roundhouse","Version":"0.1.6","Type":"Full","FileName":"b","SHA1":"","Size":1,"WorkerVersions":{"oldest":3,"newest":4}},
    {"PackageId":"Roundhouse","Version":"0.1.7","Type":"Delta","FileName":"c","SHA1":"","Size":1},
    {"PackageId":"Roundhouse","Version":"0.1.7","Type":"Full","FileName":"d","SHA1":"","Size":1,"WorkerVersions":{"oldest":4,"newest":5}}
]}"#;

/// What Velopack is given of `feed` by an app on `version` that keeps workers as old as `workers`.
fn kept_of(feed: &str, version: &str, workers: Option<u32>) -> Vec<String> {
    let releases: VelopackAssetFeed = serde_json::from_str(feed).unwrap();
    let kept = only_taking_over(
        releases,
        &serde_json::from_str::<Feed>(feed).unwrap(),
        Running { version, hub: HubRelease::On(version), workers },
    );
    kept.Assets.iter().map(|package| format!("{} {}", package.Version, package.Type)).collect()
}

/// What Velopack is given of [`SPEAKING`] while workers as old as `oldest` are kept.
fn kept_for(oldest: u32) -> Vec<String> {
    kept_of(SPEAKING, "0.1.4", Some(oldest))
}

#[test]
fn the_feed_keeps_only_releases_whose_hub_speaks_the_workers_to_keep() {
    assert_eq!(kept_for(3), ["0.1.6 Full"]);
    assert_eq!(kept_for(4), ["0.1.6 Full", "0.1.7 Delta", "0.1.7 Full"]);
    assert_eq!(kept_for(5), ["0.1.7 Delta", "0.1.7 Full"]);
    assert!(kept_for(2).is_empty(), "none speaks workers that old");
}

/// Releases that say the oldest release each updates from, after one that says none.
const UPDATING_FROM: &str = r#"{"Assets":[
    {"PackageId":"Roundhouse","Version":"0.1.6","Type":"Full","FileName":"a","SHA1":"","Size":1,"WorkerVersions":{"oldest":3,"newest":4}},
    {"PackageId":"Roundhouse","Version":"0.1.7","Type":"Delta","FileName":"b","SHA1":"","Size":1},
    {"PackageId":"Roundhouse","Version":"0.1.7","Type":"Full","FileName":"c","SHA1":"","Size":1,"WorkerVersions":{"oldest":3,"newest":4},"UpdatesFrom":"0.1.6"}
]}"#;

/// So an app that's further behind takes the release before first, which its hub can migrate the database of, whether
/// or not it has workers to keep, as on a hub whose workers stop with it.
#[test]
fn the_feed_keeps_only_releases_that_update_from_the_app_s_release() {
    for workers in [Some(3), None] {
        assert_eq!(kept_of(UPDATING_FROM, "0.1.5", workers), ["0.1.6 Full"], "{workers:?}");
        assert_eq!(
            kept_of(UPDATING_FROM, "0.1.6", workers),
            ["0.1.6 Full", "0.1.7 Delta", "0.1.7 Full"],
            "{workers:?}"
        );
    }
}

/// With no workers to keep, a release from before releases said what they speak is kept, as Velopack read it before.
#[test]
fn with_no_workers_to_keep_every_release_that_says_no_floor_is_kept() {
    assert_eq!(kept_of(SPEAKING, "0.1.4", None), ["0.1.5 Full", "0.1.6 Full", "0.1.7 Delta", "0.1.7 Full"]);
}

/// What the app knows of this computer's hub, for [`read_from`].
#[derive(Clone, Copy)]
enum Hub<'a> {
    NotHeard,
    Heard(&'a str),
    FailedToStart,
}

/// What Velopack is given of [`UPDATING_FROM`], as the source reads it, by an app on `app` that knows this of its hub.
fn read_from(app: &str, hub: Hub<'_>) -> Vec<String> {
    let (feed, signature, key) = signed(UPDATING_FROM);
    let server = serve_feed(vec![Reply::file(feed)], vec![Reply::file(signature)]);
    let keeping = ToKeep::oldest_at(None);
    match hub {
        Hub::NotHeard => {}
        Hub::Heard(hub) => _ = keeping.set(Heard { workers: None, hub: hub.to_owned() }),
        Hub::FailedToStart => _ = keeping.failed_to_start(app),
    }
    let source = SignedSource::waiting(&server.base, key, keeping, Duration::ZERO);
    let app = Manifest { version: app.parse().unwrap(), ..Manifest::default() };
    let releases = source.get_release_feed("macos-arm64", &app, "").unwrap();
    releases.Assets.iter().map(|package| format!("{} {}", package.Version, package.Type)).collect()
}

/// The app's own release, as Velopack's manifest says it, is where its update goes from, and so is its hub's, whose
/// database the update's hub migrates, while that's older. Until the app has heard from its hub, that database may be
/// older than any floor, so the update takes no release that says the oldest release it updates from.
#[test]
fn the_app_s_release_and_its_hub_s_are_where_its_update_goes_from() {
    assert_eq!(read_from("0.1.5", Hub::Heard("0.1.5")), ["0.1.6 Full"]);
    assert_eq!(read_from("0.1.6", Hub::Heard("0.1.6")), ["0.1.6 Full", "0.1.7 Delta", "0.1.7 Full"]);
    assert_eq!(read_from("0.1.6", Hub::Heard("0.1.5")), ["0.1.6 Full"], "the hub hasn't restarted on 0.1.6 yet");
    assert_eq!(read_from("0.1.6", Hub::NotHeard), ["0.1.6 Full"], "no hub heard from yet");
}

/// So a release that fixes a broken hub still arrives, though the hub's database may be older than its floor.
#[test]
fn once_its_hub_has_failed_to_start_the_app_s_update_goes_by_the_app_s_release_alone() {
    assert_eq!(read_from("0.1.6", Hub::FailedToStart), ["0.1.6 Full", "0.1.7 Delta", "0.1.7 Full"]);
    assert_eq!(read_from("0.1.5", Hub::FailedToStart), ["0.1.6 Full"], "0.1.7 updates from 0.1.6 and nothing older");
}

/// What the update took before the app heard from its hub updates from any release, so the first hub heard from is
/// more to keep only for its workers. A hub on an older release than before may have a database that an update that
/// kept the last one's can't migrate, so it's more to keep.
#[test]
fn hearing_from_a_hub_on_an_older_release_is_more_to_keep() {
    let on = |workers, hub: &str| Heard { workers: Some(workers), hub: hub.to_owned() };
    let to_keep = ToKeep::oldest_at(Some(3));
    let older_workers = ToKeep::oldest_at(Some(3));

    assert_eq!(to_keep.set(on(3, "0.1.5")), Kept::Newer, "heard from for the first time");
    assert_eq!(to_keep.set(on(3, "0.1.5")), Kept::Same);
    assert_eq!(to_keep.set(on(4, "0.1.5")), Kept::Newer);
    assert_eq!(to_keep.set(on(4, "0.1.6")), Kept::Newer, "a newer hub");
    assert_eq!(to_keep.set(on(4, "0.1.5")), Kept::Older);
    assert_eq!(older_workers.set(on(2, "0.1.5")), Kept::Older, "heard from for the first time, with older workers");
}

/// An update the app took while its hub had failed to start may not update from the hub's release, so hearing from a
/// hub on an older one than the app's drops it. A hub that fails, starts and fails again changes what's ready only as
/// far as it must.
#[test]
fn hearing_from_a_hub_older_than_the_app_after_it_failed_to_start_is_more_to_keep() {
    let on = |hub: &str| Heard { workers: Some(3), hub: hub.to_owned() };
    let failed = ToKeep::oldest_at(Some(3));
    let started_on_the_app_s = ToKeep::oldest_at(Some(3));

    assert_eq!(failed.failed_to_start("0.1.6"), Kept::Newer, "going by the app's own release");
    assert_eq!(failed.failed_to_start("0.1.6"), Kept::Same, "failing again");
    assert_eq!(failed.set(on("0.1.5")), Kept::Older, "a hub older than the app");
    assert_eq!(failed.failed_to_start("0.1.6"), Kept::Newer, "failing once it was heard from");

    started_on_the_app_s.failed_to_start("0.1.6");
    assert_eq!(started_on_the_app_s.set(on("0.1.6")), Kept::Newer, "a hub on the app's release");
    assert_eq!(started_on_the_app_s.failed_to_start("0.1.6"), Kept::Newer, "failing again");
}
