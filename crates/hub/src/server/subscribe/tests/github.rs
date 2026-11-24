//! What GitHub says of an issue or a pull request a chip follows.

use std::sync::Arc;

use github::References;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{CLIENT_PROTOCOL_VERSION, HubFrame, HubNotice};
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

/// What GitHub says of issue 12, if `frame` is a notice of its observation.
fn said_of_twelve(frame: &HubFrame) -> Option<&Observed<ReferenceSaid>> {
    let HubFrame::Notice { notice: HubNotice::Observation(Observation::Reference { reference, said, .. }), .. } = frame
    else {
        return None;
    };
    (*reference == twelve()).then_some(said)
}

/// What GitHub says of issue 12, if `frame` is a notice of its observation as current.
fn current_of_twelve(frame: &HubFrame) -> Option<&ReferenceSaid> {
    let Observed::Current { value } = said_of_twelve(frame)? else { return None };
    Some(value)
}

/// A chip followed is listed first, as not known before the hub has read it, and then sent what GitHub says of it,
/// current from that read.
///
/// What the hub holds of a chip newly followed is sent at once, as of when it was read, and is current from the next
/// read the hub keeps. So where the read the start asked for is kept before the hub has taken in that the start follows
/// the chip, which a busy machine can make happen, the start is sent that read as of when it was made, and then as
/// current.
#[tokio::test]
async fn a_chip_is_listed_and_then_sent_what_github_says_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_an_issue(dir.path()).await;
    let mut client = Connection::greeted(&hub, CLIENT_PROTOCOL_VERSION).await;

    let starts = vec![start(1, Follow::Reference { reference: twelve() })];
    ok(&mut client, 1, Subscribe { stop: Vec::new(), start: starts }).await;

    client.until("the current observation", |sent| sent.iter().any(|frame| current_of_twelve(frame).is_some())).await;
    let listed = client.sent.iter().find_map(|frame| {
        let HubFrame::Listed { listed: Listed::Reference(listed), .. } = frame else { return None };
        Some(listed)
    });
    let listed = listed.expect("listed first");
    assert_eq!((&listed.reference, listed.observations.as_slice()), (&twelve(), &[][..]), "not read yet");
    let said = client.sent.iter().find_map(current_of_twelve).unwrap();
    let ReferenceSaid::Issue(issue) = said else { panic!("{said:?}") };
    assert_eq!(issue.summary.about.title, "Decide");
}
