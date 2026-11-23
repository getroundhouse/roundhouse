use protocol::client::HubNotice;
use protocol::model::{ObservationKey, Observed};

use super::super::kinds::WorkspaceLines;
use super::seen::{as_of, lines, listed, of_w, told, told_copy};
use crate::hub::tests::hub_in;

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
/// before may be old, and once nothing does, it's as of when nothing did any more.
#[tokio::test]
async fn an_observation_is_current_while_something_keeps_it_up_to_date_and_then_as_of_its_last_read() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.unwrap();
    let mut feed = hub.subscribe_feed();

    hub.observation_kept_current::<WorkspaceLines>("w".into()).await.unwrap();

    assert_eq!(told(&mut feed), [], "read before, so not current");

    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 2_000).await.unwrap();
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 3_000).await.unwrap();

    let current = told_copy(&mut feed);
    assert_eq!(current, of_w(Observed::Current { value: lines(3, 1) }, current.rev()));

    hub.observation_let_go::<WorkspaceLines>("w".into(), 5_000).await.unwrap();

    let let_go = told_copy(&mut feed);
    assert!(let_go.rev() > current.rev());
    assert_eq!(let_go, of_w(as_of(lines(3, 1), 5_000), let_go.rev()), "as of when it was let go");
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 4_000).await.unwrap();
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 6_000).await.unwrap();
    assert_eq!(told(&mut feed), [], "a read begun before it was let go is dropped, and one after says the same");
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

/// What the hub kept up to date by reading it again, as it reads GitHub while a client follows it, is as of its last
/// read once it no longer does, since nothing would have said of a change since, unlike what was watched.
#[tokio::test]
async fn an_observation_no_longer_read_again_is_as_of_its_last_read() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_kept_current::<WorkspaceLines>("w".into()).await.unwrap();
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.unwrap();
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 2_000).await.unwrap();
    let mut feed = hub.subscribe_feed();

    hub.observation_no_longer_read_again::<WorkspaceLines>("w".into()).await.unwrap();

    let no_longer = told_copy(&mut feed);
    assert_eq!(no_longer, of_w(as_of(lines(3, 1), 2_000), no_longer.rev()), "as of its last read");
    hub.observation_no_longer_read_again::<WorkspaceLines>("w".into()).await.unwrap();
    assert_eq!(told(&mut feed), [], "already as of it");
}
