//! What a client that says hello at version 5 is sent of every kind of observation, and what one at version 6 is sent
//! beside it on the same hub, which is the same, as each follows the hub, a workspace, a session and a chip.

use std::collections::BTreeMap;

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{HubFrame, HubNotice, RecordNotice};
use protocol::model::{Observation, ObservationKey, Record};

use super::kinds::{CHIP, HUB, SESSION, WORKSPACE, chip, every_kind, s, sent_at, v};
use super::making::{hub_with_a_record_of_each, make};
use crate::hub::Hub;
use crate::server::subscribe::tests::given::{mark, ok, start};
use crate::server::tests::Connection;

/// A client of version 5 and one of version 6 are each listed every kind of observation at its level, and then told
/// of it as it changes, and end holding the same records, each as the hub holds it.
#[tokio::test]
async fn a_client_of_version_5_and_one_of_version_6_are_each_sent_every_observation_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_a_record_of_each(dir.path()).await;
    for kind in every_kind() {
        make(&hub, &kind, 1).await;
    }
    let mut five = Connection::greeted(&hub, 5).await;
    let mut six = Connection::greeted(&hub, 6).await;
    let follows = vec![
        start(HUB, Follow::Hub),
        start(WORKSPACE, Follow::Workspace { id: v() }),
        start(SESSION, Follow::Session { id: s(), from: None }),
        start(CHIP, Follow::Reference { reference: chip() }),
    ];
    for client in [&mut five, &mut six] {
        ok(client, 1, Subscribe { stop: Vec::new(), start: follows.clone() }).await;
        let listed = |sent: &[HubFrame]| sent.iter().filter(|frame| matches!(frame, HubFrame::Listed { .. })).count();
        client.until("every level's list", |sent| listed(sent) == 4).await;
    }

    for kind in every_kind() {
        make(&hub, &kind, 2).await;
    }
    caught_up(&hub, &mut five, &mut six).await;

    for client in [&five, &six] {
        sent_every_observation(&hub, client).await;
    }
    assert_eq!(records(&five), records(&six), "the same records");
}

/// Waits until each client has been sent what the hub told before, at each level it follows, since each subscription is
/// sent its own on its own: a notice at the hub's level, as [`mark`] waits, a subagent of `s` at its session's, what
/// [`v`] changes read again at its own, and the chip's observation as it is now.
async fn caught_up(hub: &Hub, five: &mut Connection, six: &mut Connection) {
    let changes = ObservationKey::WorkspaceChanges { workspace_id: v() };
    make(hub, &changes, 3).await;
    let read_again = held(hub, &changes).await;
    let chip = held(hub, &ObservationKey::Reference { reference: chip() }).await;
    hub.notify(super::super::given::subagent_made(hub));
    for client in [five, six] {
        mark(hub, client).await;
        let subagent = |(at, notice): (u64, &HubNotice)| at == SESSION && matches!(notice, HubNotice::Subagent(_));
        client.until("the subagent", |sent| notices_in(sent).any(subagent)).await;
        client.until("the changes read again", |sent| told_in(sent, WORKSPACE, &read_again)).await;
        client.until("the chip as it is now", |sent| told_in(sent, CHIP, &chip)).await;
    }
}

/// The observation `kind` as the hub holds it now.
async fn held(hub: &Hub, kind: &ObservationKey) -> Observation {
    let (now, _) = hub.observations_at(&kind.level().unwrap()).await.unwrap();
    now.into_iter().find(|observation| observation.key() == *kind).unwrap()
}

/// Whether `sent` tells of `observation` at subscription `at`.
fn told_in(sent: &[HubFrame], at: u64, observation: &Observation) -> bool {
    let of = |(to, notice): (u64, &HubNotice)| to == at && *notice == HubNotice::Observation(observation.clone());
    notices_in(sent).any(of)
}

/// `client` is listed each kind at the subscription of its level, and told of it there, last as the hub holds it now,
/// and is sent no observation anywhere else.
async fn sent_every_observation(hub: &Hub, client: &Connection) {
    for kind in every_kind() {
        let Some(at) = sent_at(&kind) else {
            assert!(observations(client).all(|(_, observation)| observation.key() != kind), "{kind:?} sent");
            continue;
        };
        assert!(listed(client, at).any(|observation| observation.key() == kind), "{kind:?} not listed");
        let now = held(hub, &kind).await;
        assert_eq!(told(client, at, &kind).last().copied(), Some(&now), "{kind:?} not told as it is now");
    }
    for (at, observation) in observations(client) {
        assert_eq!(Some(at), sent_at(&observation.key()), "{observation:?} on another level's subscription");
    }
}

/// Each notice in `sent`, with the subscription it was sent on.
fn notices_in(sent: &[HubFrame]) -> impl Iterator<Item = (u64, &HubNotice)> {
    sent.iter().filter_map(|frame| {
        let HubFrame::Notice { subscription_id, notice } = frame else { return None };
        Some((*subscription_id, notice))
    })
}

/// Each record `client` holds once it has taken in what it was sent, listed and then told, in order, by its key, but
/// for the marker, which [`mark`] tells each client in turn under a revision of its own.
fn records(client: &Connection) -> BTreeMap<String, Record> {
    let mut held = BTreeMap::new();
    for frame in &client.sent {
        match frame {
            HubFrame::Listed { listed: Listed::Hub(records), .. } => {
                held.extend(
                    records.as_ref().clone().into_records().map(|record| (format!("{:?}", record.key()), record)),
                );
            }
            HubFrame::Notice { notice, .. } => match RecordNotice::try_from(notice.clone()) {
                Ok(RecordNotice::Copy(record)) => {
                    held.insert(format!("{:?}", record.key()), record);
                }
                Ok(RecordNotice::Removed { record, .. }) => {
                    held.remove(&format!("{record:?}"));
                }
                Err(_) => {}
            },
            HubFrame::Listed { .. }
            | HubFrame::Reply { .. }
            | HubFrame::Event { .. }
            | HubFrame::Tail { .. }
            | HubFrame::Delta { .. }
            | HubFrame::Terminal { .. }
            | HubFrame::TerminalPicture { .. } => {}
        }
    }
    held.retain(|_, record| !matches!(record, Record::Comment(_)));
    held
}

/// Each observation `client` was listed at subscription `at`.
fn listed(client: &Connection, at: u64) -> impl Iterator<Item = &Observation> {
    client.sent.iter().flat_map(move |frame| -> &[Observation] {
        let HubFrame::Listed { subscription_id, listed } = frame else { return &[] };
        if *subscription_id != at {
            return &[];
        }
        match listed {
            Listed::Hub(records) => &records.observations,
            Listed::Session(records) => &records.observations,
            Listed::Workspace(listed) => &listed.observations,
            Listed::Reference(listed) => &listed.observations,
            Listed::Unknown => &[],
        }
    })
}

/// Each copy of the observation `kind` `client` was told of at subscription `at`, in order.
fn told<'a>(client: &'a Connection, at: u64, kind: &ObservationKey) -> Vec<&'a Observation> {
    let of_kind = notices_in(&client.sent).filter_map(|(subscription_id, notice)| {
        let HubNotice::Observation(observation) = notice else { return None };
        (subscription_id == at && observation.key() == *kind).then_some(observation)
    });
    of_kind.collect()
}

/// Each observation `client` was sent, listed or told, with the subscription it was sent on.
fn observations(client: &Connection) -> impl Iterator<Item = (u64, &Observation)> {
    let listed = [HUB, WORKSPACE, SESSION, CHIP].into_iter().flat_map(|at| listed(client, at).map(move |o| (at, o)));
    let told = notices_in(&client.sent).filter_map(|(at, notice)| {
        let HubNotice::Observation(observation) = notice else { return None };
        Some((at, observation))
    });
    listed.chain(told)
}
