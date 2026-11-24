//! The release the update feed offers a hub, which a client of version 4 reads on the record `HubUpdate`, and a client
//! of the version that reads every observation reads as an observation at the hub's level.

use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{EVERY_OBSERVATION_FROM, HubFrame, HubNotice};
use protocol::model::{HubUpdate, Observation, ReleaseOffer};

use super::accounts::mark;
use super::given::{ok, start};
use crate::hub::ReleaseOffered;
use crate::hub::tests::{hub_in, store_of};
use crate::server::tests::Connection;

/// Each copy of `HubUpdate` `client` was sent, as the release it says.
fn offered_on_the_record(client: &Connection) -> Vec<Option<String>> {
    let copies = client.sent.iter().filter_map(|frame| {
        let HubFrame::Notice { notice: HubNotice::HubUpdate(update), .. } = frame else { return None };
        Some(update.version.clone())
    });
    copies.collect()
}

/// How many observations of the release offered `client` was sent.
fn observed(client: &Connection) -> usize {
    let told = |frame: &&HubFrame| {
        matches!(frame, HubFrame::Notice { notice: HubNotice::Observation(Observation::ReleaseOffered { .. }), .. })
    };
    client.sent.iter().filter(told).count()
}

/// The record a client of version 4 is sent says the release the observation does, as the record leaves the hub, and
/// only a client of the version that reads every observation is sent the observation.
#[tokio::test]
async fn the_release_offered_reaches_each_client_in_the_form_its_version_reads() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut before = Connection::greeted(&hub, 4).await;
    let mut observing = Connection::greeted(&hub, EVERY_OBSERVATION_FROM).await;
    for client in [&mut before, &mut observing] {
        ok(client, 1, Subscribe { stop: Vec::new(), start: vec![start(1, Follow::Hub)] }).await;
        client.until("the list", |sent| sent.iter().any(|frame| matches!(frame, HubFrame::Listed { .. }))).await;
    }

    let offer = ReleaseOffer { version: Some("0.1.9".into()) };
    hub.observation_read::<ReleaseOffered>((), offer, 1_000).await.unwrap();
    // As the hub tells of its read, which made the record, at a newer revision.
    let rev = store_of(&hub).next_rev().unwrap();
    let record = HubUpdate { version: None, looked_at_ms: 1_000, problem: None, attempt: None, rev };
    hub.notify(HubNotice::HubUpdate(record));
    for client in [&mut before, &mut observing] {
        mark(&hub, client).await;
    }

    assert_eq!(offered_on_the_record(&before), [Some("0.1.9".to_string())]);
    assert_eq!((observed(&before), observed(&observing)), (0, 1));
}
