//! What GitHub says of an app of your own, which a client of version 4 reads on the app's record, and a client of the
//! version that reads every observation reads as an observation, on a hub whose database is from before it left the
//! record.

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{EVERY_OBSERVATION_FROM, HubFrame, HubNotice};
use protocol::model::{
    DiffComment, DiffSide, GithubApp, GithubAppId, GithubAppSaid, GithubUser, GithubUserId, Observation, Rev,
};

use super::given::{ok, start};
use crate::hub::GithubAppSaid as Said;
use crate::hub::tests::hub_in_again;
use crate::server::tests::Connection;
use crate::store::BEFORE_APPS_OBSERVED;
use crate::store::older::store_from_before;

const APP: GithubAppId = GithubAppId::new(9);

/// The app's record, as the hub's level lists it to `client`, and the observations listed with it.
fn listed_app(client: &Connection) -> (GithubApp, Vec<Observation>) {
    let records = client.sent.iter().find_map(|frame| {
        let HubFrame::Listed { listed: Listed::Hub(records), .. } = frame else { return None };
        Some(records)
    });
    let records = records.expect("the hub's level listed");
    (records.github_apps[0].clone(), records.observations.clone())
}

/// Each copy of the app's record `client` was sent.
fn apps_sent(client: &Connection) -> Vec<GithubApp> {
    let copies = client.sent.iter().filter_map(|frame| {
        let HubFrame::Notice { notice: HubNotice::GithubApp(app), .. } = frame else { return None };
        Some(app.clone())
    });
    copies.collect()
}

/// Whether `sent` tells what GitHub says of the app.
fn told_what_github_says(sent: &[HubFrame]) -> bool {
    sent.iter().any(|frame| {
        matches!(frame, HubFrame::Notice { notice: HubNotice::Observation(Observation::GithubAppSaid { .. }), .. })
    })
}

/// A hub on a database from before what GitHub says of an app left it lists the app to a client of version 4 named
/// by its number, and to one of the version that reads every observation with no observation of it. What GitHub says
/// next fills it: the first client is sent the record again, named, under a newer revision, and the second the
/// observation, and no copy of the record that moves only that.
#[tokio::test]
async fn what_github_says_of_an_app_reaches_each_client_in_the_form_its_version_reads() {
    let dir = tempfile::tempdir().unwrap();
    let rows = "INSERT INTO github_apps (id, slug, name, owner_id, owner, key_file, made_from)
                    VALUES (9, 'ada-roundhouse', 'Ada', 2, 'adalabs', 'a-file', 'a-code');";
    drop(store_from_before(dir.path(), BEFORE_APPS_OBSERVED, rows));
    let hub = hub_in_again(dir.path());
    let mut before = Connection::greeted(&hub, 4).await;
    let mut observing = Connection::speaking(&hub, EVERY_OBSERVATION_FROM);
    for client in [&mut before, &mut observing] {
        ok(client, 1, Subscribe { stop: Vec::new(), start: vec![start(1, Follow::Hub)] }).await;
        client.until("the list", |sent| sent.iter().any(|frame| matches!(frame, HubFrame::Listed { .. }))).await;
    }
    let (listed, observations) = listed_app(&before);
    assert_eq!((listed.slug.as_str(), listed.name.as_str()), ("#9", "#9"), "not known, so named by its number");
    assert!(observations.is_empty() && listed_app(&observing).1.is_empty(), "{observations:?}");

    let owner = GithubUser { id: GithubUserId::new(2), name: "adalabs".into() };
    let said = GithubAppSaid { slug: "ada-roundhouse".into(), name: "Ada's".into(), owner };
    hub.observation_read::<Said>(APP, said, 1_000).await.unwrap();
    let marker = HubNotice::Comment(DiffComment {
        id: "c".into(),
        workspace_id: "w".into(),
        path: "a.rs".into(),
        side: DiffSide::New,
        line: 1,
        text: "after".into(),
        created_at_ms: 1,
        // A copy with no revision is always sent.
        rev: Rev::default(),
    });
    hub.notify(marker.clone());
    for client in [&mut before, &mut observing] {
        let at_the_marker = |sent: &[HubFrame]| {
            sent.iter().any(|frame| matches!(frame, HubFrame::Notice { notice, .. } if *notice == marker))
        };
        client.until("the marker", at_the_marker).await;
    }

    let copies = apps_sent(&before);
    let [copy] = copies.as_slice() else { panic!("{copies:?}") };
    assert_eq!((copy.slug.as_str(), copy.name.as_str()), ("ada-roundhouse", "Ada's"));
    assert!(copy.rev > listed.rev, "{:?} after {:?}", copy.rev, listed.rev);
    assert!(!told_what_github_says(&before.sent), "no observation to a client of version 4");
    assert!(told_what_github_says(&observing.sent));
    assert_eq!(apps_sent(&observing), [], "no copy that moves only what GitHub says");
}
