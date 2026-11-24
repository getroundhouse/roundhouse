//! What a client that says hello at version 5 is sent of every kind of observation, and what one at version 4 is sent
//! beside it on the same hub, as each follows the hub, a workspace, a session and a chip.

use std::sync::Arc;

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{EVERY_OBSERVATION_FROM, HubFrame, HubNotice, RecordNotice};
use protocol::model::{Observation, ObservationKey, Record, RecordKey, Rev};
use tokio::sync::broadcast;

use super::kinds::{Before, CHIP, HUB, SESSION, Sent, WORKSPACE, chip, every_kind, s, sent, v};
use super::making::{hub_with_a_record_of_each, make};
use crate::hub::{Fed, Hub, Moves};
use crate::server::subscribe::tests::accounts::mark;
use crate::server::subscribe::tests::given::{ok, start};
use crate::server::tests::Connection;

/// A client of version 5 is listed every kind of observation at its level, and then told of it as it changes, and is
/// sent no `GithubUpdate`, no copy of a record that moves only an observation's old form, and no `GithubReach`, though
/// its starts don't say they read GitHub as observations. One of version 4 beside it is sent each old form instead: the
/// record's field filled, under a newer revision as the observation changes, `GithubReach`, and `GithubUpdate`, and no
/// observation but those every version reads.
#[tokio::test]
async fn a_client_of_version_5_is_sent_every_observation_and_one_of_version_4_each_old_form() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_a_record_of_each(dir.path()).await;
    for kind in every_kind() {
        make(&hub, &kind, 1).await;
    }
    let mut fed = hub.subscribe_fed();
    let mut observing = Connection::greeted(&hub, EVERY_OBSERVATION_FROM).await;
    let mut before = Connection::greeted(&hub, 4).await;
    let follows = vec![
        start(HUB, Follow::Hub),
        start(WORKSPACE, Follow::Workspace { id: v() }),
        start(SESSION, Follow::Session { id: s(), from: None }),
        start(CHIP, Follow::Reference { reference: chip() }),
    ];
    for (client, lists) in [(&mut observing, 4), (&mut before, 3)] {
        ok(client, 1, Subscribe { stop: Vec::new(), start: follows.clone() }).await;
        let listed = |sent: &[HubFrame]| sent.iter().filter(|frame| matches!(frame, HubFrame::Listed { .. })).count();
        client.until("every level's list", |sent| listed(sent) == lists).await;
    }

    // Each in turn, once the client of version 4 has the record that holds the one before it filled, so no change to
    // another kind on the same record fills it in its place.
    for kind in every_kind() {
        make(&hub, &kind, 2).await;
        if let Before::Record { record, says } = sent(&kind).before {
            let filled = |(_, copy): (bool, Record)| copy.key() == record && says(&copy).as_deref() == Some("2");
            before.until("its old form filled", |sent| records_in(sent).any(filled)).await;
        }
    }
    caught_up(&hub, &mut observing, &mut before).await;

    let old_form_only = old_form_only(&mut fed);
    sent_every_observation(&hub, &observing).await;
    sent_no_old_form(&observing, &old_form_only);
    sent_each_old_form(&before, &old_form_only);
}

/// Waits until each client has been sent what the hub told before, at each level it follows, since each subscription is
/// sent its own on its own: a notice at the hub's level, as [`mark`] waits, a subagent of `s` at its session's, what
/// [`v`] changes read again at its own, and, to the client of version 5, the chip's observation as it is now.
async fn caught_up(hub: &Hub, observing: &mut Connection, before: &mut Connection) {
    let changes = ObservationKey::WorkspaceChanges { workspace_id: v() };
    make(hub, &changes, 3).await;
    let read_again = held(hub, &changes).await;
    let chip = held(hub, &ObservationKey::Reference { reference: chip() }).await;
    hub.notify(super::super::given::subagent_made(hub));
    for client in [&mut *observing, &mut *before] {
        mark(hub, client).await;
        let subagent = |(at, notice): (u64, &HubNotice)| at == SESSION && matches!(notice, HubNotice::Subagent(_));
        client.until("the subagent", |sent| notices_in(sent).any(subagent)).await;
        client.until("the changes read again", |sent| told_in(sent, WORKSPACE, &read_again)).await;
    }
    observing.until("the chip as it is now", |sent| told_in(sent, CHIP, &chip)).await;
    // Sent as each subscription starts.
    for on in [HUB, CHIP] {
        before.until("what GitHub says", |sent| github_in(sent, on)).await;
    }
}

/// Whether `sent` has a `GithubUpdate` on subscription `on`.
fn github_in(sent: &[HubFrame], on: u64) -> bool {
    sent.iter().any(|frame| matches!(frame, HubFrame::Github { subscription_id, .. } if *subscription_id == on))
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

/// A client of version 5 is listed each kind at the subscription of its level, and told of it there, last as the hub
/// holds it now, and is sent no observation anywhere else.
async fn sent_every_observation(hub: &Hub, client: &Connection) {
    for kind in every_kind() {
        let Sent { at, .. } = sent(&kind);
        let Some(at) = at else {
            assert!(observations(client).all(|(_, observation)| observation.key() != kind), "{kind:?} sent");
            continue;
        };
        assert!(listed(client, at).any(|observation| observation.key() == kind), "{kind:?} not listed");
        let now = held(hub, &kind).await;
        assert_eq!(told(client, at, &kind).last().copied(), Some(&now), "{kind:?} not told as it is now");
    }
    for (at, observation) in observations(client) {
        assert_eq!(Some(at), sent(&observation.key()).at, "{observation:?} on another level's subscription");
    }
}

/// A client of version 5 is sent no `GithubUpdate`, no `GithubReach`, and none of `old_form_only`, the copies of
/// records the feed said move only an observation's old form.
fn sent_no_old_form(client: &Connection, old_form_only: &[Arc<HubNotice>]) {
    let told = |notice: &HubNotice| old_form_only.iter().any(|old| **old == *notice);
    let moving_old_forms: Vec<_> = notices(client).filter(|(_, notice)| told(notice)).collect();
    assert!(moving_old_forms.is_empty(), "copies that move only an old form: {moving_old_forms:?}");
    assert!(!client.sent.iter().any(|frame| matches!(frame, HubFrame::Github { .. })), "a GithubUpdate");
    let reach = |record: &Record| matches!(record, Record::GithubReach(_));
    assert!(!records(client).any(|(_, record)| reach(&record)), "a GithubReach");
}

/// A client of version 4 is sent every copy of `old_form_only`, each old form filled with what its observation says
/// as each changes, under a newer revision, `GithubUpdate` where its start follows what GitHub says, and only the
/// observations every version reads.
fn sent_each_old_form(client: &Connection, old_form_only: &[Arc<HubNotice>]) {
    assert!(!old_form_only.is_empty());
    for old in old_form_only {
        assert!(notices(client).any(|(_, notice)| notice == &**old), "{old:?} not sent");
    }
    for kind in every_kind() {
        match sent(&kind).before {
            Before::Itself => {
                let at = sent(&kind).at.unwrap();
                assert!(listed(client, at).any(|observation| observation.key() == kind), "{kind:?} not listed");
                assert!(!told(client, at, &kind).is_empty(), "{kind:?} not told");
            }
            Before::GithubUpdate { on } => assert!(github_in(&client.sent, on), "{kind:?}: no GithubUpdate"),
            Before::Record { record, says } => filled(client, &kind, &record, says),
        }
    }
    for (_, observation) in observations(client) {
        let before = sent(&observation.key()).before;
        assert!(matches!(before, Before::Itself), "{observation:?} sent to a client of version 4");
    }
}

/// Each copy of `record` `client` was sent, listed or told, says what `kind` was made to say as it was sent: the
/// first round's, in a list made after it, and the second's last, under a revision newer than any copy of the first.
fn filled(client: &Connection, kind: &ObservationKey, record: &RecordKey, says: fn(&Record) -> Option<String>) {
    let copies: Vec<(bool, Rev, Option<String>)> = records(client)
        .filter(|(_, copy)| copy.key() == *record)
        .map(|(listed, copy)| (listed, copy.rev(), says(&copy)))
        .collect();
    let round = |round: &str, (_, _, said): &&(bool, Rev, Option<String>)| said.as_deref() == Some(round);
    assert!(copies.iter().filter(|(listed, _, _)| *listed).all(|copy| round("1", &copy)), "{kind:?}: {copies:?}");
    assert!(copies.last().is_some_and(|copy| round("2", &copy)), "{kind:?} not filled: {copies:?}");
    let first = copies.iter().filter(|copy| round("1", copy)).map(|(_, rev, _)| *rev).max();
    let second = copies.iter().filter(|copy| round("2", copy)).map(|(_, rev, _)| *rev).min();
    assert!(first < second, "{kind:?} filled under no newer revision: {copies:?}");
}

/// The copies of records the feed told, from `fed`, that move only an observation's old form, but for any it told the
/// same of as moving more, as a read of a workspace's pull request that records it again can, under the revision that an
/// old form's move took, which a client of any version is sent.
fn old_form_only(fed: &mut broadcast::Receiver<Fed>) -> Vec<Arc<HubNotice>> {
    let mut told = Vec::new();
    while let Ok(fed) = fed.try_recv() {
        told.push(fed);
    }
    let moving_more =
        |notice: &HubNotice| told.iter().any(|fed| fed.moves == Moves::Anything && *fed.notice == *notice);
    let old_form_only = told.iter().filter(|fed| fed.moves == Moves::OldFormOnly && !moving_more(&fed.notice));
    old_form_only.map(|fed| fed.notice.clone()).collect()
}

/// Each notice `client` was sent, with the subscription it was sent on.
fn notices(client: &Connection) -> impl Iterator<Item = (u64, &HubNotice)> {
    notices_in(&client.sent)
}

/// Each notice in `sent`, with the subscription it was sent on.
fn notices_in(sent: &[HubFrame]) -> impl Iterator<Item = (u64, &HubNotice)> {
    sent.iter().filter_map(|frame| {
        let HubFrame::Notice { subscription_id, notice } = frame else { return None };
        Some((*subscription_id, notice))
    })
}

/// Each copy of a record `client` was sent, in order, and whether it was listed or told.
fn records(client: &Connection) -> impl Iterator<Item = (bool, Record)> {
    records_in(&client.sent)
}

/// Each copy of a record in `sent`, in order, and whether it was listed or told.
fn records_in(sent: &[HubFrame]) -> impl Iterator<Item = (bool, Record)> {
    sent.iter().flat_map(|frame| -> Vec<(bool, Record)> {
        match frame {
            HubFrame::Listed { listed: Listed::Hub(records), .. } => {
                records.as_ref().clone().into_records().map(|record| (true, record)).collect()
            }
            HubFrame::Notice { notice, .. } => match RecordNotice::try_from(notice.clone()) {
                Ok(RecordNotice::Copy(record)) => vec![(false, record)],
                Ok(RecordNotice::Removed { .. }) | Err(_) => Vec::new(),
            },
            HubFrame::Listed { .. }
            | HubFrame::Reply { .. }
            | HubFrame::Event { .. }
            | HubFrame::Tail { .. }
            | HubFrame::Delta { .. }
            | HubFrame::Github { .. }
            | HubFrame::Terminal { .. }
            | HubFrame::TerminalPicture { .. } => Vec::new(),
        }
    })
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
    let of_kind = notices(client).filter_map(|(subscription_id, notice)| {
        let HubNotice::Observation(observation) = notice else { return None };
        (subscription_id == at && observation.key() == *kind).then_some(observation)
    });
    of_kind.collect()
}

/// Each observation `client` was sent, listed or told, with the subscription it was sent on.
fn observations(client: &Connection) -> impl Iterator<Item = (u64, &Observation)> {
    let listed = [HUB, WORKSPACE, SESSION, CHIP].into_iter().flat_map(|at| listed(client, at).map(move |o| (at, o)));
    let told = notices(client).filter_map(|(at, notice)| {
        let HubNotice::Observation(observation) = notice else { return None };
        Some((at, observation))
    });
    listed.chain(told)
}
