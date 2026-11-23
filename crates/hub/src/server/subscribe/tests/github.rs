//! What GitHub says of an issue or a pull request a chip follows, sent in the one form each subscription reads.

use std::sync::Arc;

use github::References;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::{FeedSubscribe, Follow, Subscribe, Subscription};
use protocol::client::{HubFrame, HubNotice};
use protocol::model::{
    About, GithubRepositoryName, Issue, IssueNumber, IssueState, IssueSummary, Observation, Observed, Read, Reference,
    ReferenceAsk, ReferenceSaid,
};

use super::given::{ok, start};
use crate::hub::Hub;
use crate::hub::tests::hub_on_github_with;
use crate::server::tests::Connection;

/// The issue numbered 12 in `example/project`, the repository the hub's test workspace is on.
fn twelve() -> ReferenceAsk {
    ReferenceAsk {
        repository: GithubRepositoryName::named("example/project"),
        number: IssueNumber::try_from(12).unwrap(),
    }
}

/// A hub on GitHub's stand-in, where issue 12 is open and titled `Decide`, working out what's sent of GitHub as it
/// serves.
async fn hub_with_an_issue(dir: &std::path::Path) -> Arc<Hub> {
    let about = About {
        url: "https://github.com/example/project/issues/12".into(),
        title: "Decide".into(),
        author: None,
        opened_at_ms: 1,
        comments: 0,
        labels: Vec::new(),
        body: String::new(),
    };
    let references = References::default();
    references.set(Reference::Issue(Issue {
        repository: GithubRepositoryName::named("example/project"),
        number: IssueNumber::try_from(12).unwrap(),
        summary: Read { said: IssueSummary { about, state: IssueState::Open }, read_at_ms: 1 },
    }));
    let (hub, _, _) = hub_on_github_with(dir, &references).await;
    tokio::spawn(hub.clone().observe_github());
    hub
}

/// A start that reads GitHub as observations.
fn observing(subscription_id: u64, follow: Follow) -> Subscription {
    Subscription { github_observed: true, ..start(subscription_id, follow) }
}

/// What GitHub says of issue 12, if `frame` is a notice of its observation.
fn said_of_twelve(frame: &HubFrame) -> Option<&Observed<ReferenceSaid>> {
    let HubFrame::Notice { notice: HubNotice::Observation(Observation::Reference { reference, said, .. }), .. } = frame
    else {
        return None;
    };
    (*reference == twelve()).then_some(said)
}

/// Whether `frame` is of GitHub's old form.
fn github(frame: &HubFrame) -> bool {
    matches!(frame, HubFrame::Github { .. })
}

/// A chip followed by a start that reads GitHub as observations is listed first, as not known before the hub has read
/// it, and then sent what GitHub says of it, current from that read, and never `HubFrame::Github`. One followed by a
/// start that doesn't, on the same connection, is sent `HubFrame::Github` alone, as before observations.
#[tokio::test]
async fn a_chip_is_sent_as_an_observation_only_to_a_start_that_reads_github_so() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_an_issue(dir.path()).await;
    let mut client = Connection::greeted(&hub, 4).await;
    let chip = Follow::Reference { reference: twelve() };

    let starts = vec![observing(1, chip.clone()), start(2, chip)];
    ok(&mut client, 1, Subscribe { stop: Vec::new(), start: starts }).await;

    client
        .until("the observation and the update", |sent| {
            sent.iter().any(|frame| said_of_twelve(frame).is_some())
                && sent.iter().any(|frame| matches!(frame, HubFrame::Github { subscription_id: 2, .. }))
        })
        .await;
    let listed = client.sent.iter().find_map(|frame| {
        let HubFrame::Listed { subscription_id, listed: Listed::Reference(listed) } = frame else { return None };
        Some((*subscription_id, listed))
    });
    let (1, listed) = listed.expect("listed first") else { panic!("listed to the start that doesn't read them") };
    assert_eq!((&listed.reference, listed.observations.as_slice()), (&twelve(), &[][..]), "not read yet");
    let said = client.sent.iter().find_map(said_of_twelve).unwrap();
    let Observed::Current { value: ReferenceSaid::Issue(issue) } = said else { panic!("{said:?}") };
    assert_eq!(issue.summary.about.title, "Decide");
    let to = |frame: &HubFrame| match frame {
        HubFrame::Notice { subscription_id, .. } | HubFrame::Github { subscription_id, .. } => Some(*subscription_id),
        HubFrame::Reply { .. }
        | HubFrame::Event { .. }
        | HubFrame::Tail { .. }
        | HubFrame::Delta { .. }
        | HubFrame::Terminal { .. }
        | HubFrame::TerminalPicture { .. }
        | HubFrame::Listed { .. } => None,
    };
    assert!(client.sent.iter().filter(|frame| github(frame)).all(|frame| to(frame) == Some(2)));
    assert!(client.sent.iter().filter(|frame| said_of_twelve(frame).is_some()).all(|frame| to(frame) == Some(1)));
}

/// A client of version 3, which follows the feed, is sent no observation of a chip that another client follows, since
/// it reads what GitHub says as `HubFrame::Github`.
#[tokio::test]
async fn the_feed_sends_no_observation_of_github() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_an_issue(dir.path()).await;
    let mut before = Connection::greeted(&hub, 3).await;
    ok(&mut before, 1, FeedSubscribe { subscription_id: 1 }).await;
    let mut observing_client = Connection::greeted(&hub, 4).await;

    let starts = vec![observing(1, Follow::Reference { reference: twelve() })];
    ok(&mut observing_client, 1, Subscribe { stop: Vec::new(), start: starts }).await;
    observing_client.until("the observation", |sent| sent.iter().any(|frame| said_of_twelve(frame).is_some())).await;
    // Of the hub's level, after the observation, so once the feed's subscriber has it, it would have the observation.
    hub.notify(HubNotice::Subagent(protocol::fixtures::subagent("c")));

    before
        .until("the notice after it", |sent| {
            sent.iter().any(|frame| matches!(frame, HubFrame::Notice { notice: HubNotice::Subagent(_), .. }))
        })
        .await;
    assert!(!before.sent.iter().any(|frame| matches!(
        frame,
        HubFrame::Notice { notice: HubNotice::Observation(_) | HubNotice::ObservationRemoved { .. }, .. }
    )));
}

/// A start at the hub's level that reads GitHub as observations is still sent each workspace's pull request as
/// `HubFrame::Github`, since the hub sends none of it as an observation yet.
#[tokio::test]
async fn the_hub_s_level_still_sends_github_s_updates() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_an_issue(dir.path()).await;
    let mut client = Connection::greeted(&hub, 4).await;

    ok(&mut client, 1, Subscribe { stop: Vec::new(), start: vec![observing(1, Follow::Hub)] }).await;

    client.until("the workspace's pull request", |sent| sent.iter().any(github)).await;
}
