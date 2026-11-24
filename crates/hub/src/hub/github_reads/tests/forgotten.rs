//! The pull request of an archived workspace, which nothing reads in turn, read again by the hub's timed look while a
//! client follows the hub, once the cache has forgotten it, and worked out into the workspace's observations, and when
//! one that couldn't be read is tried again.

use std::sync::Arc;
use std::time::Duration;

use github::{PullRequestState, References};
use protocol::model::{GithubRepositoryName, IssueNumber, OwnPullRequestDetail, OwnPullRequestSaid, PrState};

use super::super::Forgotten;
use crate::hub::observations::{WorkspacePullRequest, WorkspacePullRequestDetail};
use crate::hub::tests::{archived, forget_github, forget_github_but_a_summary, hub_on_github_with};
use crate::hub::{Followed, Hub};

/// Long enough for anything here on a busy machine, many times the test timers' pace.
const WAIT: Duration = Duration::from_secs(10);

/// A hub whose workspace `w` has pull request 7, read while it was open, and is archived, with the pull request merged
/// since. It works out what's sent of GitHub, and looks at what clients follow on the test timers' pace.
async fn archived_with_a_merged_pull_request(
    dir: &std::path::Path,
) -> (Arc<Hub>, github::PullRequests, github::StandIn) {
    let (hub, pull_requests, github) = hub_on_github_with(dir, &References::default()).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.workspace_pull_request("w".into()).await.unwrap();
    archived(&hub);
    pull_requests.set(7, PullRequestState::Merged);
    tokio::spawn(hub.clone().observe_github());
    (hub, pull_requests, github)
}

/// Where `w`'s observation says its pull request stands, if it says.
fn stands(hub: &Hub) -> Option<PrState> {
    let said: OwnPullRequestSaid = hub.observation_value::<WorkspacePullRequest>(&"w".into())?;
    Some(said.pull_request?.pull_request.summary.state)
}

/// Whether `w`'s observation of its pull request's detail has its checks.
fn has_checks(hub: &Hub) -> bool {
    let detail: Option<OwnPullRequestDetail> = hub.observation_value::<WorkspacePullRequestDetail>(&"w".into());
    detail.and_then(|detail| detail.pull_request).is_some_and(|pull_request| pull_request.checks.is_some())
}

/// The number of the pull request the record of the workspace `w` links to.
async fn linked_to(hub: &Hub) -> Option<u64> {
    let workspace = hub.workspace(&"w".into()).await.unwrap();
    workspace.pull_request_linked().map(|link| link.number.get())
}

/// One the cache has forgotten, as after it was emptied, is read by the number its record links to while a client
/// follows the hub, and the workspace's observation says it as GitHub does now. The record keeps its link.
#[tokio::test]
async fn an_archived_workspaces_pull_request_the_cache_forgot_is_read_again_while_the_hub_is_followed() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _pull_requests, _github) = archived_with_a_merged_pull_request(dir.path()).await;
    forget_github(&hub);
    let _following = hub.follow([Followed::Hub]);

    tokio::spawn(hub.clone().watch_followed_references());

    waiting::task::until(WAIT, "the pull request, merged", || stands(&hub) == Some(PrState::Merged)).await;
    assert_eq!(linked_to(&hub).await, Some(7), "the record keeps its link");
}

/// One the cache has only the summary of, as a chip's read keeps it, is read again too, since what the bar shows needs
/// its checks, and the workspace's observations say it whole.
#[tokio::test]
async fn an_archived_workspaces_pull_request_the_cache_has_only_the_summary_of_is_read_again() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _pull_requests, _github) = archived_with_a_merged_pull_request(dir.path()).await;
    forget_github_but_a_summary(&hub);
    let _following = hub.follow([Followed::Hub]);

    tokio::spawn(hub.clone().watch_followed_references());

    let whole = || stands(&hub) == Some(PrState::Merged) && has_checks(&hub);
    waiting::task::until(WAIT, "the pull request, merged, with its checks", whole).await;
}

/// One whose first read fails, as while GitHub is down, is tried again while a client follows the hub, until it's
/// read.
#[tokio::test]
async fn an_archived_workspaces_pull_request_that_couldnt_be_read_again_is_tried_again() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = archived_with_a_merged_pull_request(dir.path()).await;
    forget_github(&hub);
    pull_requests.fail(true);
    let asked = github.asked().len();
    let _following = hub.follow([Followed::Hub]);

    tokio::spawn(hub.clone().watch_followed_references());
    waiting::task::until(WAIT, "a read that fails", || github.asked().len() > asked).await;
    pull_requests.fail(false);

    waiting::task::until(WAIT, "the pull request, merged", || stands(&hub) == Some(PrState::Merged)).await;
    assert_eq!(linked_to(&hub).await, Some(7), "the record keeps its link");
}

/// A pull request the cache forgot is tried again only once the wait since it was last tried has passed, however many
/// ask, while another is tried at once, and the wait is twice as long after each try, up to the longest.
#[test]
fn a_forgotten_pull_request_is_tried_again_only_once_its_due_and_less_often_each_time() {
    let link = |n: u64| protocol::model::PullRequestLink {
        repository: GithubRepositoryName::named("o/r"),
        number: IssueNumber::try_from(n).unwrap(),
        url: format!("https://github.com/o/r/pull/{n}"),
    };
    let forgotten = Forgotten::default();
    let (again, start) = (Duration::from_secs(60), std::time::Instant::now());
    let due = |after: u64, numbers: &[u64]| {
        let links = numbers.iter().map(|n| link(*n)).collect();
        let due = forgotten.due(links, again, start + Duration::from_secs(after));
        due.iter().map(|link| link.number.get()).collect::<Vec<_>>()
    };

    assert_eq!(due(0, &[7, 7]), [7], "once, however many ask");
    assert_eq!(due(59, &[7, 8]), [8], "not 7 again yet");
    assert_eq!(due(60, &[7, 8]), [7], "7 is due, and 8 isn't yet");
    assert_eq!(due(60 + 119, &[7]), Vec::<u64>::new(), "twice as long after the second try");
    assert_eq!(due(60 + 120, &[7]), [7]);
    let mut at = 180;
    for wait in [240, 480, 960, 1800, 1800] {
        assert_eq!(due(at + wait - 1, &[7]), Vec::<u64>::new(), "not before {wait}s");
        assert_eq!(due(at + wait, &[7]), [7], "after {wait}s, half an hour at the longest");
        at += wait;
    }
}
