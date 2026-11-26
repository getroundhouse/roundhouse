use protocol::client::HubNotice;
use protocol::model::{Observation, ObservationKey, Observed, ReleaseOffer};

use super::super::ReleaseOffered;
use super::super::keepers::Keeper;
use super::super::kinds::WorkspaceLines;
use super::seen::{as_of, lines, listed, of_w, told, told_copy};
use crate::hub::tests::hub_in;
use crate::store::now_ms;
use crate::worker_link::WorkerLink;

/// A read that finds what's kept tells clients nothing and moves no revision, however much later it's made, and one that
/// finds something else tells them, under a newer revision than the list they hold. One that began before the read
/// kept, and lands after it, changes nothing.
#[tokio::test]
async fn only_a_read_that_finds_something_else_tells_clients_and_moves_the_revision() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut feed = hub.subscribe_feed();
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.unwrap();
    let first = told_copy(&mut feed);
    let (before, list_rev) = listed(&hub).await;
    assert_eq!(before, std::slice::from_ref(&first));
    assert_eq!(first, of_w(as_of(lines(3, 1), 1_000), first.rev()));

    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 2_000).await.unwrap();

    assert_eq!(told(&mut feed), []);
    assert_eq!(listed(&hub).await, (before, list_rev), "as of the read that found it, at the same revision");

    hub.observation_read::<WorkspaceLines>("w".into(), lines(4, 1), 3_000).await.unwrap();
    hub.observation_read::<WorkspaceLines>("w".into(), lines(5, 0), 2_500).await.unwrap();

    let changed = told_copy(&mut feed);
    assert!(changed.rev() > list_rev, "{changed:?} after a list at {list_rev}");
    assert_eq!(changed, of_w(as_of(lines(4, 1), 3_000), changed.rev()));
    assert_eq!(listed(&hub).await.0, [changed], "the late read left it");
}

/// What something keeps up to date is current from the first read after it starts, not before, since what was read
/// before may be old. Once what kept it lets go, though it stays, it's as its kind says, which for lines is as of when
/// it let go, and another letting go of what it doesn't keep changes nothing.
#[tokio::test]
async fn an_observation_is_current_while_something_keeps_it_and_as_its_kind_says_once_that_lets_go() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.unwrap();
    let mut feed = hub.subscribe_feed();

    hub.observation_kept_by::<WorkspaceLines>("w".into(), Keeper::Hub).await.unwrap();

    assert_eq!(told(&mut feed), [], "read before, so not current");

    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 2_000).await.unwrap();
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 3_000).await.unwrap();

    let current = told_copy(&mut feed);
    assert_eq!(current, of_w(Observed::Current { value: lines(3, 1) }, current.rev()));

    let another = Keeper::Worker(WorkerLink::unconnected().id);
    hub.observation_no_longer_kept_by::<WorkspaceLines>("w".into(), another, now_ms()).await.unwrap();
    assert_eq!(told(&mut feed), [], "what another doesn't keep is left as it was");
    let before = now_ms();
    hub.observation_no_longer_kept_by::<WorkspaceLines>("w".into(), Keeper::Hub, now_ms()).await.unwrap();

    let let_go = told_copy(&mut feed);
    assert!(let_go.rev() > current.rev());
    let Observation::WorkspaceLines { lines: Observed::AsOf { value, as_of_ms }, .. } = let_go else { panic!() };
    assert_eq!(value, lines(3, 1));
    assert!(as_of_ms >= before, "as of when it was let go, as lines are once unkept");
    hub.observation_no_longer_kept_by::<WorkspaceLines>("w".into(), Keeper::Hub, now_ms()).await.unwrap();
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 4_000).await.unwrap();
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), as_of_ms + 1_000).await.unwrap();
    assert_eq!(
        told(&mut feed),
        [],
        "saying it again, a read begun before, and one after that finds the same tell none"
    );
}

/// Once what an observation is of is gone, clients are told it's gone, and it's listed no more. Saying so again, or of
/// what was never read, tells them nothing.
#[tokio::test]
async fn an_observation_of_what_is_gone_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.unwrap();
    let (_, list_rev) = listed(&hub).await;
    let mut feed = hub.subscribe_feed();

    hub.observation_gone::<WorkspaceLines>("w".into()).await.unwrap();
    hub.observation_gone::<WorkspaceLines>("w".into()).await.unwrap();
    hub.observation_gone::<WorkspaceLines>("never-read".into()).await.unwrap();

    let told = told(&mut feed);
    let [HubNotice::ObservationRemoved { observation, rev }] = told.as_slice() else {
        panic!("not one removal: {told:?}")
    };
    assert_eq!(*observation, ObservationKey::WorkspaceLines { workspace_id: "w".into() });
    assert!(*rev > list_rev);
    assert_eq!(listed(&hub).await.0, []);
}

/// What's worked out from what's kept now replaces what was worked out before, even where what it's worked out from was
/// read earlier, as when it's of another pull request now. It's said as of when that was read, and working out the same
/// again tells nothing.
#[tokio::test]
async fn what_is_worked_out_replaces_what_was_though_it_was_read_earlier() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_worked_out::<WorkspaceLines>("w".into(), lines(3, 1), 2_000).await.unwrap();
    let mut feed = hub.subscribe_feed();

    hub.observation_worked_out::<WorkspaceLines>("w".into(), lines(4, 0), 1_000).await.unwrap();

    let changed = told_copy(&mut feed);
    assert_eq!(changed, of_w(as_of(lines(4, 0), 1_000), changed.rev()));
    hub.observation_worked_out::<WorkspaceLines>("w".into(), lines(4, 0), 1_000).await.unwrap();
    hub.observation_worked_out::<WorkspaceLines>("w".into(), lines(4, 0), 3_000).await.unwrap();
    assert_eq!(told(&mut feed), [], "the same, worked out again");
    assert_eq!(listed(&hub).await.0, [changed], "as of the one that found it");
}

/// A part of it can change with no read, so it stays as of the read, and what was never read isn't made up.
#[tokio::test]
async fn what_is_worked_out_again_stays_as_of_its_read_and_is_nothing_where_nothing_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut feed = hub.subscribe_feed();
    let another = |kept: &mut _| *kept = lines(4, 0);

    hub.observation_worked_out_again::<WorkspaceLines>("w".into(), another).await.unwrap();
    assert_eq!(told(&mut feed), []);
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.unwrap();
    let read = told_copy(&mut feed);
    hub.observation_worked_out_again::<WorkspaceLines>("w".into(), another).await.unwrap();

    let changed = told_copy(&mut feed);
    assert_eq!(changed, of_w(as_of(lines(4, 0), 1_000), changed.rev()));
    assert!(changed.rev() > read.rev());
    hub.observation_worked_out_again::<WorkspaceLines>("w".into(), another).await.unwrap();
    assert_eq!(told(&mut feed), [], "the same, worked out again");
}

/// What the hub kept up to date by reading it again, as it reads the update feed, is as of its last read once it no
/// longer does, as its kind says, since nothing would have said of a change since, unlike what was watched.
#[tokio::test]
async fn an_observation_of_a_kind_read_again_is_as_of_its_last_read_once_no_longer_kept() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let offered = ReleaseOffer { version: Some("1.2.0".into()) };
    hub.observation_kept_by::<ReleaseOffered>((), Keeper::Hub).await.unwrap();
    hub.observation_read::<ReleaseOffered>((), offered.clone(), 1_000).await.unwrap();
    hub.observation_read::<ReleaseOffered>((), offered.clone(), 2_000).await.unwrap();
    let mut feed = hub.subscribe_feed();

    hub.observation_no_longer_kept_by::<ReleaseOffered>((), Keeper::Hub, now_ms()).await.unwrap();

    let no_longer = told_copy(&mut feed);
    let as_of_its_last_read = Observed::AsOf { value: offered, as_of_ms: 2_000 };
    assert_eq!(no_longer, Observation::ReleaseOffered { offered: as_of_its_last_read, rev: no_longer.rev() });
    hub.observation_no_longer_kept_by::<ReleaseOffered>((), Keeper::Hub, now_ms()).await.unwrap();
    assert_eq!(told(&mut feed), [], "already as of it");
}
