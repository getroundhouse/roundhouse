use std::time::Duration;

use fetching::serving::{Reply, serve};
use protocol::client::HubNotice;
use protocol::model::HubUpdate;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair as _};
use update_feed::UpdateFeedKey;

use super::UpdateFeed;
use crate::hub::stand_in_workers::TIMEOUT;
use crate::hub::tests::hub_in;
use crate::versions::WORKERS;

/// A full package, with the archive of its hub if `archive`, whose hub speaks the workers this build's hub starts.
fn release(version: &str, archive: bool) -> String {
    let speaks = format!(r#"{{"oldest":{},"newest":{}}}"#, WORKERS.newest, WORKERS.newest);
    let hub =
        format!(r#","Hub":{{"FileName":"Roundhouse-{version}-osx-hub.tar.gz","Size":9,"SHA1":"A","SHA256":"B"}}"#);
    let hub = if archive { hub.as_str() } else { "" };
    format!(r#"{{"Version":"{version}","Type":"Full","WorkerVersions":{speaks}{hub}}}"#)
}

/// The feed of `releases`, served with its signature by a key of its own, and where the hub reads it. `signed_by` is
/// the key the hub checks it with, or else the one that signed it.
fn served(releases: &[String], signed_by: Option<UpdateFeedKey>) -> UpdateFeed {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let feed = format!(r#"{{"Assets":[{}]}}"#, releases.join(","));
    let signature = pair.sign(feed.as_bytes()).as_ref().to_vec();
    let paths = vec![
        ("releases.osx.json".to_owned(), vec![Reply::file(feed)]),
        ("releases.osx.json.sig".to_owned(), vec![Reply::file(signature)]),
    ];
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

    hub.look_for_update(&feed, "0.1.8", Duration::ZERO).await;

    let told = tokio::time::timeout(TIMEOUT, async {
        loop {
            if let Ok(HubNotice::HubUpdate(update)) = notices.recv().await {
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

    hub.look_for_update(&feed, "0.1.9", Duration::ZERO).await;

    let update = listed(&hub).await.expect("the hub read the feed");
    assert_eq!((update.version, update.problem), (None, None));
}

/// A feed whose signature doesn't match may not be the release workflow's, so nothing is taken from it, and what the
/// read before found stands.
#[tokio::test]
async fn a_feed_signed_with_another_key_changes_nothing_but_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.look_for_update(&served(&[release("0.1.9", true)], None), "0.1.8", Duration::ZERO).await;
    let before = listed(&hub).await.unwrap();
    let another = served(&[release("0.1.9", true)], None).key;

    let planted = served(&[release("0.1.9", true), release("0.1.20", true)], Some(another));
    hub.look_for_update(&planted, "0.1.8", Duration::ZERO).await;

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
