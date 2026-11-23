//! The hub's observations, as the store keeps them, sent to each subscription of the level they're sent at.

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe, Subscribed};
use protocol::client::{ClientResponse, HubFrame, HubNotice};
use protocol::model::{LineCounts, Observation, ObservationKey, Observed};

use super::given::{Given, ok, start};
use crate::hub::WorkspaceLines;
use crate::server::tests::Connection;

/// The subscriptions' IDs: one that follows the hub, and one that follows the session `s`.
const HUB: u64 = 1;
const SESSION: u64 = 2;

/// A client that follows the hub is listed a workspace's lines the store keeps, under the list's revision, and is sent
/// each change to them and their removal. One that follows only a session is sent none of it, since a workspace's lines
/// are sent at the hub's level.
#[tokio::test]
async fn a_workspace_s_lines_are_listed_and_sent_to_whoever_follows_the_hub_and_no_other() {
    let dir = tempfile::tempdir().unwrap();
    let given = Given::new(dir.path()).await;
    let hub = &given.hub;
    let lines = |added, removed| LineCounts { added, removed };
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.unwrap();
    let mut one = Connection::greeted(hub, 4).await;
    let session = Follow::Session { id: "s".into(), from: None };
    let follows = Subscribe { stop: Vec::new(), start: vec![start(HUB, Follow::Hub), start(SESSION, session)] };
    let subscribed = ok(&mut one, 1, follows).await;
    one.until("both lists", |sent| sent.iter().filter(|frame| matches!(frame, HubFrame::Listed { .. })).count() == 2)
        .await;

    hub.observation_read::<WorkspaceLines>("w".into(), lines(4, 0), 2_000).await.unwrap();
    hub.observation_gone::<WorkspaceLines>("w".into()).await.unwrap();

    // Of `s`, after the rest, so once it's sent on the session's subscription, whatever that was sent before it was too.
    hub.notify(HubNotice::Subagent(protocol::fixtures::subagent("c")));

    let of_s = |frame: &HubFrame| {
        matches!(frame, HubFrame::Notice { subscription_id: SESSION, notice: HubNotice::Subagent(_) })
    };
    one.until("the removal, and the notice of s after it", |sent| {
        sent.iter().any(|frame| observation_notice(frame).is_some_and(is_removal)) && sent.iter().any(of_s)
    })
    .await;
    assert_eq!(subscribed, ClientResponse::Subscribed(Subscribed::default()));
    let records = one.sent.iter().find_map(listed).unwrap();
    let [kept] = records.observations.as_slice() else { panic!("{:?}", records.observations) };
    let as_of = |value, as_of_ms| Observed::AsOf { value, as_of_ms };
    assert_eq!(
        *kept,
        Observation::WorkspaceLines { workspace_id: "w".into(), lines: as_of(lines(3, 1), 1_000), rev: kept.rev() }
    );
    assert!(records.rev >= kept.rev());
    let told: Vec<_> = one.sent.iter().filter_map(observation_notice).collect();
    let [(HUB, HubNotice::Observation(changed)), (HUB, HubNotice::ObservationRemoved { observation, rev })] =
        told.as_slice()
    else {
        panic!("{told:?}")
    };
    assert_eq!(
        *changed,
        Observation::WorkspaceLines { workspace_id: "w".into(), lines: as_of(lines(4, 0), 2_000), rev: changed.rev() }
    );
    assert!(changed.rev() > records.rev && *rev > changed.rev());
    assert_eq!(*observation, ObservationKey::WorkspaceLines { workspace_id: "w".into() });
}

/// The hub's records, if `frame` lists them.
fn listed(frame: &HubFrame) -> Option<&protocol::model::Records> {
    let HubFrame::Listed { subscription_id: HUB, listed: Listed::Hub(records) } = frame else { return None };
    Some(records)
}

/// A notice of an observation, with the subscription that carried it.
fn observation_notice(frame: &HubFrame) -> Option<(u64, &HubNotice)> {
    let HubFrame::Notice { subscription_id, notice } = frame else { return None };
    matches!(notice, HubNotice::Observation(_) | HubNotice::ObservationRemoved { .. })
        .then_some((*subscription_id, notice))
}

fn is_removal((_, notice): (u64, &HubNotice)) -> bool {
    matches!(notice, HubNotice::ObservationRemoved { .. })
}
