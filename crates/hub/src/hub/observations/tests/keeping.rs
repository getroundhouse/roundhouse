//! What keeps an observation, and what each kind is once that goes.

use protocol::client::HubNotice;
use protocol::model::{Observed, OtherBuild, ReleaseOffer, Rev, RunId};

use super::super::keepers::Keeper;
use super::super::kept::Said;
use super::super::kinds::{Kind, SessionKey, SessionToolServers, WorkspaceLines};
use super::super::{ReleaseOffered, WorkspaceWorkerBuild};
use super::seen::{lines, told};
use crate::hub::Hub;
use crate::hub::tests::hub_in;
use crate::store::now_ms;
use crate::worker_link::WorkerLink;

/// What the store keeps of the observation of kind `K` of `of`, as a client is sent it.
fn observed<K: Kind>(hub: &Hub, of: &K::Of) -> Option<Observed<K::Value>> {
    K::table(&mut hub.observations.tables.lock()).said(of).map(Said::observed)
}

/// Whatever keeps several kinds lets go of every one with one call as it goes, each as its kind says: a worker's watch
/// of a worktree's lines is as of when it went, which build it is is gone, and what's as of its last read once unkept
/// is as of that read. What another keeps is left as it was, and clients are told of each that changed.
#[tokio::test]
async fn whatever_keeps_observations_lets_go_of_every_kind_as_each_says_as_it_goes() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let (worker, another) =
        (Keeper::Worker(WorkerLink::unconnected().id), Keeper::Worker(WorkerLink::unconnected().id));
    let build = OtherBuild { said: Some("0.9.0".into()) };
    let offer = ReleaseOffer { version: None };
    hub.observation_said_by_while::<WorkspaceLines>("w".into(), worker, lines(3, 1), 1_000, || true).await.unwrap();
    hub.observation_said_by_while::<WorkspaceWorkerBuild>("w".into(), worker, build, 1_000, || true).await.unwrap();
    hub.observation_said_by_while::<ReleaseOffered>((), worker, offer.clone(), 1_000, || true).await.unwrap();
    hub.observation_said_by_while::<WorkspaceLines>("v".into(), another, lines(1, 0), 1_000, || true).await.unwrap();
    let (mut feed, before) = (hub.subscribe_feed(), now_ms());

    hub.keeper_went(worker).await;

    let Some(Observed::AsOf { value, as_of_ms }) = observed::<WorkspaceLines>(&hub, &"w".into()) else { panic!() };
    assert_eq!(value, lines(3, 1));
    assert!(as_of_ms >= before, "as of when it went, not of the read");
    assert_eq!(observed::<WorkspaceWorkerBuild>(&hub, &"w".into()), None);
    assert_eq!(observed::<ReleaseOffered>(&hub, &()), Some(Observed::AsOf { value: offer, as_of_ms: 1_000 }));
    assert_eq!(observed::<WorkspaceLines>(&hub, &"v".into()), Some(Observed::Current { value: lines(1, 0) }));
    let told = told(&mut feed);
    assert_eq!(told.len(), 3, "{told:?}");
    let revs: Vec<_> = told.iter().filter_map(rev_of).collect();
    assert!(revs.windows(2).all(|pair| pair[0] == pair[1]), "one revision for each: {revs:?}");
}

/// The revision a notice of an observation is at.
fn rev_of(notice: &HubNotice) -> Option<Rev> {
    if let HubNotice::ObservationRemoved { rev, .. } = notice {
        return Some(*rev);
    }
    let HubNotice::Observation(observation) = notice else { return None };
    Some(observation.rev())
}

/// What a run says is never current, though the run goes on, and what's only true of it goes as another run keeps it
/// in its place, so a late word from the first, or its going, leaves the next's. Nothing is kept of a word that's no
/// longer wanted.
#[tokio::test]
async fn what_only_a_run_keeps_goes_as_another_keeps_it_and_its_late_word_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let of = SessionKey { workspace_id: "w".into(), session_id: "s".into() };
    let (first, next) = (Keeper::Run(RunId::new(1)), Keeper::Run(RunId::new(2)));
    let servers = |of: &SessionKey| observed::<SessionToolServers>(&hub, of);

    hub.observation_said_by_while::<SessionToolServers>(of.clone(), first, Vec::new(), 1_000, || true).await.unwrap();
    assert_eq!(servers(&of), Some(Observed::AsOf { value: Vec::new(), as_of_ms: 1_000 }), "never current");

    hub.observation_kept_by::<SessionToolServers>(of.clone(), next).await.unwrap();
    assert_eq!(servers(&of), None, "gone as the next run keeps them");

    hub.observation_said_by_while::<SessionToolServers>(of.clone(), next, Vec::new(), 2_000, || true).await.unwrap();
    hub.observation_said_by_while::<SessionToolServers>(of.clone(), first, Vec::new(), 3_000, || false).await.unwrap();
    hub.keeper_went(first).await;
    assert_eq!(servers(&of), Some(Observed::AsOf { value: Vec::new(), as_of_ms: 2_000 }), "the next's, as it said");

    hub.keeper_went(next).await;
    assert_eq!(servers(&of), None);
}
