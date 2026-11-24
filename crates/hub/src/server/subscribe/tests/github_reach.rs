//! Whether the default can push to a repository, which a client of version 4 reads as the record `GithubReach`, made
//! from the observation, and a client of the version that reads every observation reads as the observation.

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{EVERY_OBSERVATION_FROM, HubFrame, HubNotice};
use protocol::model::{
    DiffComment, DiffSide, GithubReach, GithubRepositoryName, Observation, Observed, Reach, Records, Rev,
};

use super::given::{ok, start};
use crate::hub::RepositoryReach;
use crate::hub::tests::hub_in;
use crate::server::tests::Connection;

const CLI: GithubRepositoryName = GithubRepositoryName::named("cli/cli");

/// What the hub's level listed to `client`.
fn listed(client: &Connection) -> Records {
    let records = client.sent.iter().find_map(|frame| {
        let HubFrame::Listed { listed: Listed::Hub(records), .. } = frame else { return None };
        Some(records)
    });
    *records.expect("the hub's level listed").clone()
}

/// Each copy of a reach's record `client` was sent.
fn reaches_sent(client: &Connection) -> Vec<GithubReach> {
    let copies = client.sent.iter().filter_map(|frame| {
        let HubFrame::Notice { notice: HubNotice::GithubReach(reach), .. } = frame else { return None };
        Some(reach.clone())
    });
    copies.collect()
}

/// Each reach `client` was sent as an observation.
fn observed_sent(client: &Connection) -> Vec<Reach> {
    let copies = client.sent.iter().filter_map(|frame| {
        let HubFrame::Notice { notice: HubNotice::Observation(Observation::RepositoryReach { reach, .. }), .. } = frame
        else {
            return None;
        };
        match reach {
            Observed::Current { value } | Observed::AsOf { value, .. } => Some(value.clone()),
            Observed::NotKnown | Observed::Unknown => None,
        }
    });
    copies.collect()
}

/// A hub that holds a repository's reach lists it to a client of version 4 as its record, and to one of the version
/// that reads every observation as the observation alone. As GitHub answers otherwise, the first is sent the record
/// again, under the observation's revision, and the second the observation, and no record.
#[tokio::test]
async fn a_reach_reaches_each_client_in_the_form_its_version_reads() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let pushes = Reach::Pushes { repository: CLI, username: "ada".into() };
    let refused = Reach::Refused { repository: CLI, username: "ada".into(), reason: "no".into(), installation: None };
    hub.observation_read::<RepositoryReach>("r".into(), pushes.clone(), 1_000).await.unwrap();
    let mut before = Connection::greeted(&hub, 4).await;
    let mut observing = Connection::speaking(&hub, EVERY_OBSERVATION_FROM);
    for client in [&mut before, &mut observing] {
        ok(client, 1, Subscribe { stop: Vec::new(), start: vec![start(1, Follow::Hub)] }).await;
        client.until("the list", |sent| sent.iter().any(|frame| matches!(frame, HubFrame::Listed { .. }))).await;
    }

    hub.observation_read::<RepositoryReach>("r".into(), refused.clone(), 2_000).await.unwrap();

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
    let (as_listed, as_observed) = (listed(&before), listed(&observing));
    let [listed_reach] = as_listed.github_reach.as_slice() else { panic!("{:?}", as_listed.github_reach) };
    assert_eq!((&listed_reach.reach, listed_reach.answered_at_ms), (&pushes, Some(1_000)));
    assert!(
        as_listed.observations.iter().all(|observation| !matches!(observation, Observation::RepositoryReach { .. }))
    );
    assert_eq!(as_observed.github_reach, [], "no record made from an observation");
    assert!(
        as_observed.observations.iter().any(|observation| matches!(observation, Observation::RepositoryReach { .. }))
    );
    let copies = reaches_sent(&before);
    let [copy] = copies.as_slice() else { panic!("{copies:?}") };
    assert_eq!((&copy.reach, copy.answered_at_ms), (&refused, Some(2_000)));
    assert!(copy.rev > listed_reach.rev, "{:?} after {:?}", copy.rev, listed_reach.rev);
    assert_eq!(observed_sent(&before), [], "no observation to a client of version 4");
    assert_eq!(observed_sent(&observing), [refused]);
    assert_eq!(reaches_sent(&observing), [], "no record made from an observation");
}
