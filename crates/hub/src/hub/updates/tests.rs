use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use fetching::serving::{Reply, serve};
use protocol::client::{ErrorCode, HubNotice};
use protocol::model::{HubUpdate, Rev, UpdateAttempt};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair as _};
use update_feed::UpdateFeedKey;

use tokio::sync::broadcast::Receiver;

use super::UpdateFeed;
use super::looking::Source;
use crate::hub::stand_in_workers::TIMEOUT;
use crate::hub::tests::hub_in;
use crate::versions::WORKERS;

/// A full package, with the archive of its hub if `archive`, whose hub speaks the workers this build's hub starts.
fn release(version: &str, archive: bool) -> String {
    let speaks = format!(r#"{{"oldest":{},"newest":{}}}"#, WORKERS.newest, WORKERS.newest);
    let hub = format!(r#","Hub":{{"FileName":"{}","Size":9,"SHA1":"A","SHA256":"B"}}"#, archive_name(version));
    let hub = if archive { hub.as_str() } else { "" };
    format!(r#"{{"Version":"{version}","Type":"Full","WorkerVersions":{speaks}{hub}}}"#)
}

/// The feed of `releases`, served with its signature by a key of its own, and where the hub reads it. `signed_by` is
/// the key the hub checks it with, or else the one that signed it.
fn served(releases: &[String], signed_by: Option<UpdateFeedKey>) -> UpdateFeed {
    served_beside(releases, signed_by, Vec::new())
}

/// The same, with each of `files` served beside it, as a release's archive is. The feed is the Mac's older channel's,
/// `osx`, where a hub built before the Mac's was renamed `macos-arm64` reads, and the archives it lists are the newer
/// channel's, as `bridge` in `tools/package/publish.py` lists them, so every test here is one of such a hub updating.
fn served_beside(releases: &[String], signed_by: Option<UpdateFeedKey>, files: Vec<(String, Reply)>) -> UpdateFeed {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let feed = format!(r#"{{"Assets":[{}]}}"#, releases.join(","));
    let signature = pair.sign(feed.as_bytes()).as_ref().to_vec();
    let mut paths = vec![
        ("releases.osx.json".to_owned(), vec![Reply::file(feed)]),
        ("releases.osx.json.sig".to_owned(), vec![Reply::file(signature)]),
    ];
    paths.extend(files.into_iter().map(|(name, reply)| (name, vec![reply])));
    let server = serve(paths).unwrap();
    let key = UpdateFeedKey(pair.public_key().as_ref().try_into().unwrap());
    UpdateFeed { base: server.base.clone(), key: signed_by.unwrap_or(key), channel: "osx" }
}

/// What the hub lists, as a client starting to follow the feed reads it.
async fn listed(hub: &crate::hub::Hub) -> Option<HubUpdate> {
    hub.records_list().await.unwrap().hub_update
}

/// So every client can show the hub's update, the hub's own included, one connecting later too.
#[tokio::test]
async fn a_hub_tells_its_clients_the_release_it_would_update_to() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut notices = hub.subscribe_feed();
    let feed = served(&[release("0.1.9", true), release("0.1.10", false)], None);

    hub.look_for_update(&on(feed, "0.1.8"), Duration::ZERO).await;

    let told = tokio::time::timeout(TIMEOUT, async {
        loop {
            if let Ok(HubNotice::HubUpdate(update)) = notices.recv().await.map(std::sync::Arc::unwrap_or_clone) {
                return update;
            }
        }
    });
    let told = told.await.expect("the feed never told of the update");
    assert_eq!(told.version.as_deref(), Some("0.1.9"), "the newest with an archive");
    assert_eq!(told.problem, None);
    assert_eq!(listed(&hub).await, Some(told), "a client connecting later");
}

#[tokio::test]
async fn a_hub_on_the_newest_release_says_none_is_newer() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let feed = served(&[release("0.1.9", true)], None);

    hub.look_for_update(&on(feed, "0.1.9"), Duration::ZERO).await;

    let update = listed(&hub).await.expect("the hub read the feed");
    assert_eq!((update.version, update.problem), (None, None));
}

/// A feed whose signature doesn't match may not be the release workflow's, so nothing is taken from it, and what the
/// read before found stands.
#[tokio::test]
async fn a_feed_signed_with_another_key_changes_nothing_but_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.look_for_update(&on(served(&[release("0.1.9", true)], None), "0.1.8"), Duration::ZERO).await;
    let before = listed(&hub).await.unwrap();
    let another = served(&[release("0.1.9", true)], None).key;

    let planted = served(&[release("0.1.9", true), release("0.1.20", true)], Some(another));
    hub.look_for_update(&on(planted, "0.1.8"), Duration::ZERO).await;

    let after = listed(&hub).await.unwrap();
    assert_eq!(after.version.as_deref(), Some("0.1.9"), "as the read before found it");
    assert!(after.problem.as_deref().is_some_and(|problem| problem.contains("signature")), "{after:?}");
    assert!(after.rev.supersedes(before.rev), "a newer copy, so every client takes it");
}

/// As a hub a client started, which doesn't read the feed, so a client can tell "not known" from "nothing newer".
#[tokio::test]
async fn a_hub_that_hasnt_read_the_feed_lists_no_update() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());

    assert_eq!(listed(&hub).await, None);
}

/// The name the feed gives `version`'s archive.
fn archive_name(version: &str) -> String {
    format!("Roundhouse-{version}-macos-arm64-hub.tar.gz")
}

/// A full package of `version`, as [`release`] makes one, whose archive is `archive`.
fn release_of(version: &str, archive: &[u8]) -> String {
    let speaks = format!(r#"{{"oldest":{},"newest":{}}}"#, WORKERS.newest, WORKERS.newest);
    let mut sha256 = String::new();
    for byte in ring::digest::digest(&ring::digest::SHA256, archive).as_ref() {
        write!(sha256, "{byte:02X}").unwrap();
    }
    let (name, size) = (archive_name(version), archive.len());
    let hub = format!(r#""Hub":{{"FileName":"{name}","Size":{size},"SHA1":"A","SHA256":"{sha256}"}}"#);
    format!(r#"{{"Version":"{version}","Type":"Full","WorkerVersions":{speaks},{hub}}}"#)
}

/// An archive of a release's hub holding stand-ins for its programs: a daemon that says it's `says`, and a command
/// line that, asked to place its build, writes `placed`, where `roundhouse hub place` would place the build.
fn archive(says: &str, placed: &std::path::Path) -> Vec<u8> {
    let daemon = format!("#!/bin/sh\necho 'roundhoused {says}'\n");
    let cli = format!("#!/bin/sh\n[ \"$1 $2\" = 'hub place' ] && echo placed > '{}'\n", placed.display());
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
    for (name, script) in [("roundhoused", daemon), ("roundhouse", cli)] {
        let mut header = tar::Header::new_gnu();
        header.set_size(script.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, name, script.as_bytes()).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
}

/// The feed a hub on `running` reads, as a hub installed as a service is given it.
fn on(feed: UpdateFeed, running: &'static str) -> Source {
    Source { feed, running }
}

/// A hub on 0.1.8 that reads a feed with 0.1.9, whose archive is served as `served`, though the feed describes it as
/// `archive`, and that has read it, so it would update to it.
async fn found_update(dir: &std::path::Path, archive: &[u8], served: Reply) -> (Arc<crate::hub::Hub>, Source) {
    let hub = hub_in(dir);
    let feed = served_beside(&[release_of("0.1.9", archive)], None, vec![(archive_name("0.1.9"), served)]);
    hub.updates.read_from(feed.clone(), "0.1.8");
    let source = on(feed, "0.1.8");
    hub.look_for_update(&source, Duration::ZERO).await;
    (hub, source)
}

/// What the hub tells of next that `wanted` takes.
async fn told(notices: &mut Receiver<std::sync::Arc<HubNotice>>, wanted: impl Fn(&HubUpdate) -> bool) -> HubUpdate {
    let told = tokio::time::timeout(TIMEOUT, async {
        loop {
            if let Ok(HubNotice::HubUpdate(update)) = notices.recv().await.map(std::sync::Arc::unwrap_or_clone)
                && wanted(&update)
            {
                return update;
            }
        }
    });
    told.await.expect("the hub never told of it")
}

/// Whether the update to 0.1.9 failed, as `update` says.
fn failed(update: &HubUpdate) -> bool {
    matches!(&update.attempt, Some(UpdateAttempt::Failed { to, .. }) if to == "0.1.9")
}

fn under_way() -> UpdateAttempt {
    UpdateAttempt::UnderWay { to: "0.1.9".into() }
}

/// So the hub restarts on the release, which the service starts it on, and every client sees it's updating meanwhile.
#[cfg(unix)]
#[tokio::test]
async fn a_hub_asked_to_update_places_the_release_and_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let placed = dir.path().join("placed");
    let archive = archive("0.1.9", &placed);
    let (hub, _) = found_update(dir.path(), &archive, Reply::file(archive.clone())).await;
    let mut notices = hub.subscribe_feed();

    hub.update_to("0.1.9".into()).await.unwrap();

    let updating = told(&mut notices, |_| true).await;
    assert_eq!(updating.attempt, Some(under_way()));
    tokio::time::timeout(TIMEOUT, hub.shutdown.notified()).await.expect("the hub never went to restart");
    assert!(hub.restarting());
    assert_eq!(std::fs::read_to_string(&placed).unwrap(), "placed\n", "the release placed itself");
    assert!(!dir.path().join("service/.update").exists(), "what it downloaded is gone");
}

/// Its own request was answered as it started, so a request under way when it's done is another client's, such as
/// one making a workspace, which the restart would cut off.
#[cfg(unix)]
#[tokio::test]
async fn an_update_restarts_only_once_no_request_is_under_way() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive("0.1.9", &dir.path().join("placed"));
    let (hub, _) = found_update(dir.path(), &archive, Reply::file(archive.clone())).await;
    hub.requests.store(1, Ordering::SeqCst);

    hub.update_to("0.1.9".into()).await.unwrap();
    waiting::task::until(TIMEOUT, "the hub never placed the release", || hub.restarting()).await;
    let while_one_is = tokio::time::timeout(Duration::from_millis(300), hub.shutdown.notified()).await;
    hub.requests.store(0, Ordering::SeqCst);

    assert!(while_one_is.is_err(), "it went with a request under way");
    tokio::time::timeout(TIMEOUT, hub.shutdown.notified()).await.expect("the hub never went to restart");
}

/// The release the client showed is the one it gets, only one update runs at a time, and a request sent again after
/// its reply was lost changes nothing. The read of the feed every 10 minutes leaves the update as it is.
#[tokio::test]
async fn a_hub_updates_once_at_a_time_to_what_it_found_whatever_it_reads_meanwhile() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive("0.1.9", &dir.path().join("placed"));
    let (held, release) = Reply::file(archive.clone()).held_after(1);
    let (hub, source) = found_update(dir.path(), &archive, held).await;

    let other = hub.update_to("0.1.10".into()).await.unwrap_err();
    hub.update_to("0.1.9".into()).await.unwrap();
    hub.look_for_update(&source, Duration::ZERO).await;
    let meanwhile = listed(&hub).await.unwrap();
    let again = hub.update_to("0.1.9".into()).await;
    release.now();

    assert_eq!(other.code, ErrorCode::Conflict, "{other:?}");
    assert_eq!(meanwhile.attempt, Some(under_way()), "a read leaves the update as it is");
    assert!(meanwhile.rev.supersedes(Rev::default()));
    again.expect("sent again, it changes nothing");
}

#[tokio::test]
async fn a_hub_that_doesnt_read_the_feed_refuses_to_update() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());

    let refused = hub.update_to("0.1.9".into()).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::InvalidRequest, "{refused:?}");
}

/// An archive that isn't the one the feed's signature covers, though it's the same size, is never run, and the hub
/// carries on as it was, so every client can say the update failed, and it can be tried again.
#[tokio::test]
async fn an_archive_other_than_the_feeds_is_never_placed() {
    let dir = tempfile::tempdir().unwrap();
    let placed = dir.path().join("placed");
    let archive = archive("0.1.9", &placed);
    let mut tampered = archive.clone();
    let middle = tampered.len() / 2;
    tampered[middle] ^= 0xff;
    let (hub, _) = found_update(dir.path(), &archive, Reply::file(tampered)).await;
    let mut notices = hub.subscribe_feed();

    hub.update_to("0.1.9".into()).await.unwrap();

    let after = told(&mut notices, failed).await;
    assert_eq!(after.version.as_deref(), Some("0.1.9"), "to try again");
    assert!(!placed.exists());
    assert!(!hub.restarting());
    hub.update_to("0.1.9".into()).await.expect("tried again");
}

/// As an archive the release workflow put under the wrong release would be, which the hub would restart on as if it
/// were the one asked for.
#[cfg(unix)]
#[tokio::test]
async fn a_release_whose_daemon_is_another_build_is_never_placed() {
    let dir = tempfile::tempdir().unwrap();
    let placed = dir.path().join("placed");
    let archive = archive("0.1.10", &placed);
    let (hub, _) = found_update(dir.path(), &archive, Reply::file(archive.clone())).await;
    let mut notices = hub.subscribe_feed();

    hub.update_to("0.1.9".into()).await.unwrap();

    told(&mut notices, failed).await;
    assert!(!placed.exists());
    assert!(!hub.restarting());
}
