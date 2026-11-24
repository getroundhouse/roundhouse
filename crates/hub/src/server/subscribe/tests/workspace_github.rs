//! What GitHub says of a workspace's own pull request, sent as observations at the hub's level and the workspace's.

use std::sync::Arc;

use github::{PullRequestState, PullRequests, StandIn};
use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe, Subscription};
use protocol::client::{CLIENT_PROTOCOL_VERSION, HubFrame, HubNotice, ObservationNotice};
use protocol::model::{Observation, ObservationKey, Observed, OwnPullRequestSaid, PrState};

use super::given::{ok, start};
use crate::hub::Hub;
use crate::hub::tests::hub_on_github;
use crate::server::tests::Connection;

/// A hub on GitHub's stand-in, where the workspace `w`'s branch has pull request 7, open with a check running, working
/// out what's sent of GitHub as it serves.
async fn hub_with_a_pull_request(dir: &std::path::Path) -> (Arc<Hub>, PullRequests, StandIn) {
    let (hub, pull_requests, github) = hub_on_github(dir).await;
    pull_requests.set(7, PullRequestState::Open);
    tokio::spawn(hub.clone().observe_github());
    (hub, pull_requests, github)
}

/// A connection that has started `starts` through one `Subscribe`.
async fn following(hub: &Arc<Hub>, starts: Vec<Subscription>) -> Connection {
    let mut connection = Connection::greeted(hub, CLIENT_PROTOCOL_VERSION).await;
    ok(&mut connection, 1, Subscribe { stop: Vec::new(), start: starts }).await;
    connection
}

/// Every observation `frame` carries, in a list or a notice.
fn observations(frame: &HubFrame) -> Vec<ObservationKey> {
    let listed = |observations: &[Observation]| observations.iter().map(Observation::key).collect();
    if let HubFrame::Listed { listed: kept, .. } = frame {
        return match kept {
            Listed::Hub(records) => listed(&records.observations),
            Listed::Workspace(workspace) => listed(&workspace.observations),
            Listed::Reference(reference) => listed(&reference.observations),
            Listed::Session(_) | Listed::Unknown => Vec::new(),
        };
    }
    let HubFrame::Notice { notice, .. } = frame else { return Vec::new() };
    ObservationNotice::try_from(notice.clone()).ok().map(|told| told.key()).into_iter().collect()
}

/// How `w`'s pull request stands, with the revision, if `frame` is a notice of it.
fn stands(frame: &HubFrame) -> Option<(PrState, u64)> {
    let HubFrame::Notice {
        notice: HubNotice::Observation(Observation::WorkspacePullRequest { workspace_id, pull_request, rev }),
        ..
    } = frame
    else {
        return None;
    };
    let (Observed::Current { value } | Observed::AsOf { value, .. }) = pull_request else { return None };
    let OwnPullRequestSaid { pull_request: Some(own) } = value else { return None };
    (workspace_id.as_str() == "w").then(|| (own.pull_request.summary.state, rev.get()))
}

/// The revision of the hub's list `frame` is, if it is one.
fn listed_rev(frame: &HubFrame) -> Option<u64> {
    let HubFrame::Listed { listed: Listed::Hub(records), .. } = frame else { return None };
    Some(records.rev.get())
}

/// A notice of the hub's level, after whatever was sent before it, so a client that has it would have that too.
fn after_the_rest(hub: &Hub) {
    hub.notify(super::given::comment_made(hub, "why"));
}

fn has_the_rest(sent: &[HubFrame]) -> bool {
    sent.iter().any(|frame| matches!(frame, HubFrame::Notice { notice: HubNotice::Comment(_), .. }))
}

/// Two clients that follow the hub are each sent how a workspace's pull request stands as GitHub changes it, under a
/// revision newer than the one before and than their list.
#[tokio::test]
async fn two_clients_that_follow_the_hub_are_both_sent_the_newer_summary_by_its_revision() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_with_a_pull_request(dir.path()).await;
    let mut one = following(&hub, vec![start(1, Follow::Hub)]).await;
    let mut two = following(&hub, vec![start(1, Follow::Hub)]).await;
    for client in [&mut one, &mut two] {
        client.until("its list", |sent| sent.iter().any(|frame| listed_rev(frame).is_some())).await;
    }

    hub.workspace_pull_request("w".into()).await.unwrap();
    pull_requests.set(7, PullRequestState::Merged);
    hub.workspace_pull_request("w".into()).await.unwrap();

    for client in [&mut one, &mut two] {
        let merged = |sent: &[HubFrame]| sent.iter().filter_map(stands).any(|(state, _)| state == PrState::Merged);
        client.until("the pull request merged", merged).await;
        let told: Vec<_> = client.sent.iter().filter_map(stands).collect();
        let [(PrState::Open, open), (PrState::Merged, merged)] = told.as_slice() else { panic!("{told:?}") };
        let list = client.sent.iter().find_map(listed_rev).unwrap();
        assert!(*merged > *open && *open > list, "{list}, {open}, {merged}");
    }
}

/// A client that follows the hub alone is sent how a workspace's pull request stands, and no checks, while one that
/// follows the workspace too is sent its checks at the workspace's level.
#[tokio::test]
async fn a_client_that_follows_the_hub_alone_is_sent_no_checks() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _pull_requests, _github) = hub_with_a_pull_request(dir.path()).await;
    hub.workspace_pull_request("w".into()).await.unwrap();
    let mut hub_alone = following(&hub, vec![start(1, Follow::Hub)]).await;
    let mut both = following(&hub, vec![start(1, Follow::Hub), start(2, Follow::Workspace { id: "w".into() })]).await;

    let detail = |frame: &HubFrame| {
        let HubFrame::Listed { listed: Listed::Workspace(workspace), .. } = frame else { return None };
        workspace.observations.iter().find_map(|observation| {
            let Observation::WorkspacePullRequestDetail { detail, .. } = observation else { return None };
            Some(detail.clone())
        })
    };
    both.until("the workspace's list", |sent| sent.iter().any(|frame| detail(frame).is_some())).await;
    after_the_rest(&hub);
    hub_alone.until("the notice after the rest", has_the_rest).await;

    let checks = both.sent.iter().find_map(detail).unwrap();
    let (Observed::Current { value } | Observed::AsOf { value, .. }) = checks else { panic!("{checks:?}") };
    assert!(value.pull_request.and_then(|detail| detail.checks).is_some_and(|checks| !checks.checks.is_empty()));
    let sent_alone: Vec<_> = hub_alone.sent.iter().flat_map(observations).collect();
    assert!(sent_alone.contains(&ObservationKey::WorkspacePullRequest { workspace_id: "w".into() }), "{sent_alone:?}");
    assert!(!sent_alone.iter().any(|key| matches!(key, ObservationKey::WorkspacePullRequestDetail { .. })));
}
