//! What GitHub says of a workspace's own pull request, worked out as observations from the workspace's record and what
//! the cache keeps, with a stand-in for GitHub where the workspace `w`'s branch has pull request 7.

use std::sync::Arc;
use std::time::Duration;

use github::{PullRequestState, PullRequests, StandIn};
use protocol::client::{HubNotice, ObservationNotice};
use protocol::model::{
    GithubRepositoryName, IssueNumber, Observation, ObservationKey, Observed, OwnPullRequestDetail, OwnPullRequestSaid,
    PrState, PullRequest, PullRequestLink, PullRequestLinked, RecordKey, Reference, ReferenceAsk, RepositoryUnseen,
};
use tokio::sync::broadcast;

use crate::hub::observations::WorkspacePullRequest;
use crate::hub::tests::hub_on_github;
use crate::hub::{Followed, Hub};

/// Long enough for anything here on a busy machine.
const WAIT: Duration = Duration::from_secs(10);

/// A hub on GitHub's stand-in, working out what's sent of GitHub as it serves, with what answers for pull requests.
async fn observing(dir: &std::path::Path) -> (Arc<Hub>, PullRequests, StandIn) {
    let (hub, pull_requests, github) = hub_on_github(dir).await;
    tokio::spawn(hub.clone().observe_github());
    (hub, pull_requests, github)
}

/// The next notice the feed tells of an observation of `w` that `of` picks out.
async fn next<T>(
    feed: &mut broadcast::Receiver<std::sync::Arc<HubNotice>>,
    of: impl Fn(ObservationNotice) -> Option<T>,
) -> T {
    let next = async {
        loop {
            if let Ok(notice) = ObservationNotice::try_from(std::sync::Arc::unwrap_or_clone(feed.recv().await.unwrap()))
                && let Some(picked) = of(notice)
            {
                return picked;
            }
        }
    };
    tokio::time::timeout(WAIT, next).await.expect("the feed told of it")
}

/// A copy of how `w`'s pull request stands, as it says it, with its revision.
fn summary(notice: ObservationNotice) -> Option<(Observed<OwnPullRequestSaid>, u64)> {
    let ObservationNotice::Copy(Observation::WorkspacePullRequest { workspace_id, pull_request, rev }) = notice else {
        return None;
    };
    (workspace_id.as_str() == "w").then(|| (pull_request, rev.get()))
}

/// A copy of `w`'s pull request's detail, as it says it.
fn detail(notice: ObservationNotice) -> Option<Observed<OwnPullRequestDetail>> {
    let ObservationNotice::Copy(Observation::WorkspacePullRequestDetail { workspace_id, detail, .. }) = notice else {
        return None;
    };
    (workspace_id.as_str() == "w").then_some(detail)
}

/// A copy of whether GitHub showed `w`'s repository.
fn unseen(notice: ObservationNotice) -> Option<Observed<Option<RepositoryUnseen>>> {
    let ObservationNotice::Copy(Observation::WorkspaceRepositoryUnseen { workspace_id, unseen, .. }) = notice else {
        return None;
    };
    (workspace_id.as_str() == "w").then_some(unseen)
}

/// The removal of an observation.
fn removed(notice: ObservationNotice) -> Option<ObservationKey> {
    let ObservationNotice::Removed { observation, .. } = notice else { return None };
    Some(observation)
}

/// Where what's said of a pull request says it stands, and whether it's current.
fn state(said: &Observed<OwnPullRequestSaid>) -> Option<(PrState, bool)> {
    let (value, current) = match said {
        Observed::Current { value } => (value, true),
        Observed::AsOf { value, .. } => (value, false),
        Observed::NotKnown | Observed::Unknown => return None,
    };
    Some((value.pull_request.as_ref()?.pull_request.summary.state, current))
}

/// A read of a workspace's pull request works out how it stands, at the hub's level, as of the read, since nothing
/// follows the workspace, and its checks, history and stack, at the workspace's, of the commit its summary is at.
#[tokio::test]
async fn a_read_works_out_how_a_workspace_s_pull_request_stands_and_its_detail() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = observing(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    let mut feed = hub.subscribe_feed();

    hub.workspace_pull_request("w".into()).await.unwrap();

    let (said, _) = next(&mut feed, summary).await;
    assert_eq!(state(&said), Some((PrState::Open, false)), "{said:?}");
    let Observed::AsOf { value: OwnPullRequestSaid { pull_request: Some(own) }, .. } = said else { panic!() };
    assert_eq!(own.pull_request.number.get(), 7);
    let detail = next(&mut feed, detail).await;
    let Observed::AsOf { value: OwnPullRequestDetail { pull_request: Some(detail) }, .. } = detail else {
        panic!("{detail:?}")
    };
    assert_eq!(detail.head, own.pull_request.summary.head);
    let checks = detail.checks.expect("its checks");
    assert_eq!(checks.checks.iter().map(|check| check.name.as_str()).collect::<Vec<_>>(), ["checks"]);
}

/// A workspace that something follows has its pull request read, and is current from that read, under a new revision
/// each time GitHub says something else of it, and none when it says the same. A read that fails makes it as of the
/// read before, until the next that doesn't, and once nothing follows it, it's as of its last read.
#[tokio::test]
async fn a_followed_workspace_s_pull_request_is_current_from_its_read_until_a_read_fails_or_nothing_follows_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = observing(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    let mut feed = hub.subscribe_feed();

    let following = hub.follow([Followed::Workspace("w".into())]);

    let (first, first_rev) = next(&mut feed, summary).await;
    assert_eq!(state(&first), Some((PrState::Open, true)), "{first:?}");
    assert!(matches!(next(&mut feed, detail).await, Observed::Current { .. }));

    hub.workspace_pull_request("w".into()).await.unwrap();
    pull_requests.set(7, PullRequestState::Merged);
    hub.workspace_pull_request("w".into()).await.unwrap();

    let (merged, merged_rev) = next(&mut feed, summary).await;
    assert_eq!(state(&merged), Some((PrState::Merged, true)), "the read that found the same told nothing");
    assert!(merged_rev > first_rev);

    pull_requests.fail(true);
    let _ = hub.workspace_pull_request("w".into()).await;

    assert_eq!(state(&next(&mut feed, summary).await.0), Some((PrState::Merged, false)), "as of the read before");

    pull_requests.fail(false);
    hub.workspace_pull_request("w".into()).await.unwrap();

    assert_eq!(state(&next(&mut feed, summary).await.0), Some((PrState::Merged, true)));

    drop(following);

    assert_eq!(state(&next(&mut feed, summary).await.0), Some((PrState::Merged, false)), "as of its last read");
}

/// GitHub showing the default no repository where the branch's pull requests are is worked out at the workspace's
/// level, and once a read sees the repository again, it says so.
#[tokio::test]
async fn a_repository_github_doesnt_show_is_worked_out_until_it_does() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = observing(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    pull_requests.hide(true);
    let mut feed = hub.subscribe_feed();

    let _ = hub.workspace_pull_request("w".into()).await;

    let hidden = next(&mut feed, unseen).await;
    assert!(matches!(hidden, Observed::AsOf { value: Some(_), .. }), "{hidden:?}");

    pull_requests.hide(false);
    hub.workspace_pull_request("w".into()).await.unwrap();

    assert!(matches!(next(&mut feed, unseen).await, Observed::AsOf { value: None, .. }));
}

/// A record that links the workspace to another pull request, which the cache has nothing of, leaves nothing known of
/// it, so its observations go until it's read. A workspace that's gone takes its observations with it, as the task
/// hears, and one worked out of it after, as by a read that raced the removal, goes once the task works everything out
/// again.
#[tokio::test]
async fn what_s_worked_out_of_a_pull_request_goes_with_its_link_and_its_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = observing(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    let mut feed = hub.subscribe_feed();
    hub.workspace_pull_request("w".into()).await.unwrap();
    next(&mut feed, detail).await;
    let link = PullRequestLink {
        repository: GithubRepositoryName::named("example/project"),
        number: IssueNumber::try_from(9).unwrap(),
        url: "https://github.com/example/project/pull/9".into(),
        uncommitted: 0,
    };
    let linked = PullRequestLinked { link: Some(link) };

    let relinked = hub.store.set_workspace_pull_request(&"w".into(), Some(&linked), None).unwrap().unwrap();
    hub.notify(HubNotice::Workspace(relinked));

    let gone = [next(&mut feed, removed).await, next(&mut feed, removed).await];
    assert_eq!(
        gone,
        [
            ObservationKey::WorkspacePullRequest { workspace_id: "w".into() },
            ObservationKey::WorkspacePullRequestDetail { workspace_id: "w".into() },
        ]
    );

    hub.store.set_workspace_pull_request(&"w".into(), Some(&PullRequestLinked { link: None }), None).unwrap();
    hub.workspace_pull_request("w".into()).await.unwrap();
    next(&mut feed, detail).await;
    // Only the notice the hub sends as it removes a workspace, since this one's chat keeps its record.
    hub.notify(HubNotice::Removed {
        record: RecordKey::Workspace { id: "w".into() },
        rev: hub.store.next_rev().unwrap(),
    });

    let gone = [next(&mut feed, removed).await, next(&mut feed, removed).await];
    assert!(gone.contains(&ObservationKey::WorkspacePullRequest { workspace_id: "w".into() }), "{gone:?}");
    assert!(gone.contains(&ObservationKey::WorkspacePullRequestDetail { workspace_id: "w".into() }), "{gone:?}");

    let worked_out = OwnPullRequestSaid { pull_request: None };
    hub.observation_worked_out::<WorkspacePullRequest>("gone".into(), worked_out, 1_000).await.unwrap();
    hub.observe_github_once().await;

    assert_eq!(next(&mut feed, removed).await, ObservationKey::WorkspacePullRequest { workspace_id: "gone".into() });
}

/// A push moves the pull request's head, which a read of its summary alone finds, as a chip's does, so the checks
/// read for the head before are of another commit: the detail is of the new head, with its checks not known until
/// they're read for it, and its history and stack as they were.
#[tokio::test]
async fn a_push_leaves_the_detail_s_checks_not_known_until_they_re_read_for_its_head() {
    let dir = tempfile::tempdir().unwrap();
    let references = github::References::default();
    let (hub, pull_requests, _github) = crate::hub::tests::hub_on_github_with(dir.path(), &references).await;
    tokio::spawn(hub.clone().observe_github());
    pull_requests.set(7, PullRequestState::Open);
    let mut feed = hub.subscribe_feed();
    hub.workspace_pull_request("w".into()).await.unwrap();
    let Observed::AsOf { value: OwnPullRequestDetail { pull_request: Some(before) }, .. } =
        next(&mut feed, detail).await
    else {
        panic!("no detail")
    };
    assert!(before.checks.is_some());
    let mut pushed = protocol::fixtures::pull_request(7);
    pushed.summary.said.head = "head-8".into();
    references.set(Reference::PullRequest(PullRequest { history: None, stack: None, checks: None, ..pushed }));

    // Older than a chip's read is kept fresh for, so it's read again.
    hub.github_cache.age(Duration::from_mins(5));
    hub.references_read(
        &"w".into(),
        vec![ReferenceAsk { repository: own(), number: IssueNumber::try_from(7).unwrap() }],
    )
    .await
    .unwrap();

    let Observed::AsOf { value: OwnPullRequestDetail { pull_request: Some(after) }, .. } =
        next(&mut feed, detail).await
    else {
        panic!("no detail")
    };
    assert_eq!(after.head.as_str(), "head-8");
    assert_eq!(after.checks, None, "the checks of head-7 aren't head-8's");
    assert_eq!((after.history, after.stack), (before.history, before.stack));
}

/// A hub that starts again has each of a workspace's three as of its read, under a revision newer than any the hub
/// before gave, and working them all out as it starts sends clients nothing, since they say what they said.
#[tokio::test]
async fn a_hub_that_starts_again_has_each_as_of_its_read_and_its_first_pass_sends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = observing(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    let mut feed = hub.subscribe_feed();
    hub.workspace_pull_request("w".into()).await.unwrap();
    next(&mut feed, detail).await;
    pull_requests.hide(true);
    let _ = hub.workspace_pull_request("w".into()).await;
    next(&mut feed, unseen).await;
    let before = hub.store.current_rev().unwrap();
    // The task that works out GitHub's observations holds the hub, so dropping it here doesn't wait for its writes.
    hub.observations_written().await;
    drop((hub, github));

    let again = crate::hub::tests::hub_in_again(dir.path());
    let mut feed = again.subscribe_feed();
    let mut kept = again.records_list().await.unwrap().observations;
    kept.extend(again.observations_at(&protocol::model::Level::Workspace { id: "w".into() }).await.unwrap().0);
    again.observe_github_once().await;

    let kinds: Vec<_> = kept.iter().map(Observation::key).collect();
    for kind in [
        ObservationKey::WorkspacePullRequest { workspace_id: "w".into() },
        ObservationKey::WorkspacePullRequestDetail { workspace_id: "w".into() },
        ObservationKey::WorkspaceRepositoryUnseen { workspace_id: "w".into() },
    ] {
        let observation = kept.iter().find(|observation| observation.key() == kind).expect("kept across the restart");
        assert!(observation.rev() > before, "{observation:?} after {before}");
        assert!(is_as_of(observation), "{observation:?}");
    }
    assert!(kinds.len() >= 3);
    let told: Vec<_> = std::iter::from_fn(|| feed.try_recv().ok().map(std::sync::Arc::unwrap_or_clone))
        .filter_map(|notice| ObservationNotice::try_from(notice).ok())
        .collect();
    assert_eq!(told, [], "the first pass found what was kept");
}

/// Whether `observation` says it's as of a read.
fn is_as_of(observation: &Observation) -> bool {
    match observation {
        Observation::WorkspacePullRequest { pull_request, .. } => matches!(pull_request, Observed::AsOf { .. }),
        Observation::WorkspacePullRequestDetail { detail, .. } => matches!(detail, Observed::AsOf { .. }),
        Observation::WorkspaceRepositoryUnseen { unseen, .. } => matches!(unseen, Observed::AsOf { .. }),
        Observation::WorkspaceLines { .. }
        | Observation::WorkspaceChanges { .. }
        | Observation::Reference { .. }
        | Observation::Unknown => false,
    }
}

/// The repository the hub's test workspace is on.
fn own() -> GithubRepositoryName {
    GithubRepositoryName::named("example/project")
}

/// A pull request that's a workspace's own and is named in a chat is read once, and the workspace's observation and the
/// chip's say the same of it, worked out from the one summary the cache keeps, each under its own freshness: current
/// for the workspace that's followed, and as of that read for the chip, which nothing has read again since.
#[tokio::test]
async fn a_pull_request_that_s_both_a_workspace_s_and_a_chip_s_is_read_once_and_both_say_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let references = github::References::default();
    let (hub, pull_requests, _github) = crate::hub::tests::hub_on_github_with(dir.path(), &references).await;
    tokio::spawn(hub.clone().observe_github());
    pull_requests.set(7, PullRequestState::Open);
    let mut feed = hub.subscribe_feed();
    let _workspace = hub.follow([Followed::Workspace("w".into())]);
    let (stands, _) = next(&mut feed, summary).await;
    let Observed::Current { value: OwnPullRequestSaid { pull_request: Some(workspace_s) } } = stands else {
        panic!("{stands:?}")
    };
    let chip = ReferenceAsk { repository: own(), number: IssueNumber::try_from(7).unwrap() };

    let _chip = hub.follow([Followed::Reference(chip.clone())]);

    let said = next(&mut feed, |notice| {
        let ObservationNotice::Copy(Observation::Reference { reference, said, .. }) = notice else { return None };
        (reference == chip).then_some(said)
    })
    .await;
    let Observed::AsOf { value: protocol::model::ReferenceSaid::PullRequest(chip_said), .. } = said else {
        panic!("{said:?}")
    };
    assert_eq!(chip_said, workspace_s.pull_request, "the chip says what the workspace's says");
    assert_eq!(references.queries(), 0, "read once, for the workspace, and not again for the chip");
}
