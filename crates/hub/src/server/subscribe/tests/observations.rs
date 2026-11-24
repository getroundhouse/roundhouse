//! The hub's observations, as the store keeps them, sent to each subscription of the level they're sent at.

use protocol::client::listed::{Listed, WorkspaceObservations};
use protocol::client::subscriptions::{Follow, Subscribe, Subscribed};
use protocol::client::{CLIENT_PROTOCOL_VERSION, ClientResponse, HubFrame, HubNotice};
use protocol::model::{LineCounts, Observation, ObservationKey, Observed};

use super::given::{Given, ok, start};
use crate::hub::tests::changes;
use crate::hub::{WorkspaceChanges, WorkspaceLines};
use crate::server::tests::{Connection, WAIT};

/// The subscriptions' IDs: one that follows the hub, one that follows the session `s`, and one that follows the
/// workspace `w`.
const HUB: u64 = 1;
const SESSION: u64 = 2;
const WORKSPACE: u64 = 3;

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
    let mut one = Connection::greeted(hub, CLIENT_PROTOCOL_VERSION).await;
    let session = Follow::Session { id: "s".into(), from: None };
    let follows = Subscribe { stop: Vec::new(), start: vec![start(HUB, Follow::Hub), start(SESSION, session)] };
    let subscribed = ok(&mut one, 1, follows).await;
    one.until("both lists", |sent| sent.iter().filter(|frame| matches!(frame, HubFrame::Listed { .. })).count() == 2)
        .await;

    hub.observation_read::<WorkspaceLines>("w".into(), lines(4, 0), 2_000).await.unwrap();
    hub.observation_gone::<WorkspaceLines>("w".into()).await.unwrap();

    // Of `s`, after the rest, so once it's sent on the session's subscription, whatever that was sent before it was too.
    hub.notify(super::given::subagent_made(hub));

    let of_s = |frame: &HubFrame| {
        matches!(frame, HubFrame::Notice { subscription_id: SESSION, notice: HubNotice::Subagent(_) })
    };
    one.until("the removal, and the notice of s after it", |sent| {
        sent.iter().any(|frame| observation_notice(frame).is_some_and(is_removal)) && sent.iter().any(of_s)
    })
    .await;
    assert_eq!(subscribed, ClientResponse::Subscribed(Subscribed::default()));
    let records = one.sent.iter().find_map(listed_hub).unwrap();
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

/// A client that follows a workspace is listed the files it changes, as the store keeps them, before the reply to its
/// `Subscribe`, and nothing of another workspace or of the hub's level, and is then sent each change to them and their
/// removal, current, since following it has the worker that serves it watch its worktree. One that follows only the
/// hub is sent none of it.
#[tokio::test]
async fn a_workspace_s_files_are_listed_before_the_reply_and_sent_only_to_whoever_follows_it() {
    let dir = tempfile::tempdir().unwrap();
    let given = Given::new(dir.path()).await;
    let hub = &given.hub;
    let lines = |added, removed| LineCounts { added, removed };
    for workspace in ["w", "v"] {
        hub.observation_read::<WorkspaceChanges>(workspace.into(), changes(3, 1), 1_000).await.unwrap();
    }
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.unwrap();
    let mut one = Connection::greeted(hub, CLIENT_PROTOCOL_VERSION).await;
    let workspace = Follow::Workspace { id: "w".into() };
    let follows = Subscribe { stop: Vec::new(), start: vec![start(HUB, Follow::Hub), start(WORKSPACE, workspace)] };

    let subscribed = ok(&mut one, 1, follows).await;
    let listed_first = one.sent.iter().find_map(workspace_listed).cloned();
    // Told on a task of its own once `w` is followed, so a read before it would be as of its time, and one after it
    // current.
    waiting::task::until(WAIT, "w's worker to be told to watch", || {
        hub.observation_is_kept_current::<WorkspaceChanges>(&"w".into())
    })
    .await;

    hub.observation_read::<WorkspaceChanges>("v".into(), changes(5, 0), 2_000).await.unwrap();
    hub.observation_read::<WorkspaceChanges>("w".into(), changes(4, 0), 2_000).await.unwrap();
    hub.observation_gone::<WorkspaceChanges>("w".into()).await.unwrap();
    // At the hub's level, after the rest, so once it's sent there, whatever that was sent before it was too.
    hub.observation_read::<WorkspaceLines>("w".into(), lines(4, 0), 2_000).await.unwrap();

    let lines_told = |sent: &[HubFrame]| sent.iter().filter_map(observation_notice).any(is_of_lines);
    one.until("the files' removal, and the lines after it", |sent| {
        sent.iter().filter_map(observation_notice).any(is_removal) && lines_told(sent)
    })
    .await;
    assert_eq!(subscribed, ClientResponse::Subscribed(Subscribed::default()));
    let listed = listed_first.expect("listed before the reply");
    let [kept] = listed.observations.as_slice() else { panic!("{listed:?}") };
    let as_of = |value, as_of_ms| Observed::AsOf { value, as_of_ms };
    let of_w = |said, rev| Observation::WorkspaceChanges { workspace_id: "w".into(), changes: said, rev };
    assert_eq!((&listed.workspace_id, kept), (&"w".into(), &of_w(as_of(changes(3, 1), 1_000), kept.rev())));
    assert!(listed.rev >= kept.rev());
    let told: Vec<_> = one.sent.iter().filter_map(observation_notice).filter(|told| !is_of_lines(*told)).collect();
    let [(WORKSPACE, HubNotice::Observation(changed)), (WORKSPACE, HubNotice::ObservationRemoved { observation, .. })] =
        told.as_slice()
    else {
        panic!("{told:?}")
    };
    assert_eq!(*changed, of_w(Observed::Current { value: changes(4, 0) }, changed.rev()));
    assert_eq!(*observation, ObservationKey::WorkspaceChanges { workspace_id: "w".into() });
    let records = one.sent.iter().find_map(listed_hub).unwrap();
    assert!(matches!(records.observations.as_slice(), [Observation::WorkspaceLines { .. }]), "{records:?}");
}

/// The observations of `w`'s level, if `frame` lists them.
fn workspace_listed(frame: &HubFrame) -> Option<&WorkspaceObservations> {
    let HubFrame::Listed { subscription_id: WORKSPACE, listed: Listed::Workspace(observations) } = frame else {
        return None;
    };
    Some(observations)
}

/// Whether a notice is of a workspace's lines.
fn is_of_lines((_, notice): (u64, &HubNotice)) -> bool {
    matches!(notice, HubNotice::Observation(Observation::WorkspaceLines { .. }))
}

/// The hub's records, if `frame` lists them.
fn listed_hub(frame: &HubFrame) -> Option<&protocol::model::Records> {
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
