//! A followed workspace's observations of GitHub are current only from a read made and answered since it was
//! followed, which only what reads it says.

use github::PullRequestState;
use protocol::client::HubNotice;
use protocol::model::{Observed, PrState};

use super::workspaces::{current_summary, next, observing, state, summary};
use crate::hub::Followed;
use crate::hub::tests::pushed;

/// A workspace followed before its first read is as of its last read until that read lands, though its record changes
/// meanwhile, as when its worker says its branches, which has what's worked out of it worked out again.
#[tokio::test]
async fn a_workspace_followed_is_as_of_its_last_read_until_the_read_following_it_lands() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = observing(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    let mut feed = hub.subscribe_feed();
    hub.workspace_pull_request("w".into()).await.unwrap();
    let (read, _) = next(&mut feed, summary).await;
    let Observed::AsOf { as_of_ms, .. } = read else { panic!("{read:?}") };
    pull_requests.hold();

    let _following = hub.follow([Followed::Workspace("w".into())]);
    let ahead = hub.store.set_workspace_branches(&"w".into(), &pushed("head-8", "head-7", (1, 0))).unwrap();
    hub.notify(HubNotice::Workspace(ahead.workspace.unwrap()));

    let (moved, _) = next(&mut feed, summary).await;
    let Observed::AsOf { value, as_of_ms: moved_as_of } = &moved else { panic!("current before its read: {moved:?}") };
    assert_eq!(value.pull_request.as_ref().unwrap().local.ahead, 1, "worked out again");
    assert_eq!(*moved_as_of, as_of_ms, "as of the read before it was followed");

    pull_requests.release();

    let (landed, _) = next(&mut feed, current_summary).await;
    assert_eq!(state(&landed), Some((PrState::Open, true)));
}
