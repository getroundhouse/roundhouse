//! Against a stand-in hub on a Unix socket.
#![cfg(unix)]

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::github::{GithubShown, GithubSubscribe, GithubUpdate};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome};
use protocol::model::{GithubRepositoryName, IssueNumber, PullRequestFound, ReferenceAsk, ReferenceRead};

use super::GithubFeed;
use crate::stand_in_hub::{self, Does};

const TIMEOUT: Duration = Duration::from_secs(10);

fn ask(n: u64) -> ReferenceAsk {
    ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(n).unwrap() }
}

fn chips(numbers: &[u64]) -> GithubShown {
    GithubShown { references: numbers.iter().map(|n| ask(*n)).collect(), ..GithubShown::default() }
}

/// A hub on `socket` that keeps each subscription to what GitHub says, and answers it with an issue missing for each
/// chip it shows, as a hub that has read them sends.
fn hub_that_says_missing(socket: &std::path::Path) -> (crate::Client, Arc<Mutex<Vec<GithubSubscribe>>>) {
    let said = Arc::new(Mutex::new(Vec::new()));
    let heard = said.clone();
    let client = stand_in_hub::connect(socket, move |request| {
        let ClientRequest::GithubSubscribe(subscribe) = request else { return Does::Nothing };
        let updates = (subscribe.shown.references.iter())
            .map(|reference| HubFrame::Github {
                subscription_id: subscribe.subscription_id,
                update: GithubUpdate::Reference { reference: reference.clone(), read: ReferenceRead::Missing },
            })
            .collect();
        heard.lock().push(subscribe);
        Does::AnswersThen(Box::new(ReplyOutcome::Response(ClientResponse::Ok)), updates)
    });
    (client, said)
}

/// What every view shows is said at once, under one subscription, again each time that changes, and what the hub
/// sends of it reaches the views that show it. What a view showed is let go of once it's gone.
#[test]
fn what_views_show_is_said_together_and_what_the_hub_sends_reaches_them() {
    let dir = tempfile::tempdir().unwrap();
    let (client, said) = hub_that_says_missing(&dir.path().join("hub.sock"));
    let feed = GithubFeed::start(&client);
    let (first, second) = (feed.view(), feed.view());

    first.show(chips(&[1, 2]));
    first.changed().recv_timeout(TIMEOUT).expect("the first view hears of what it shows");
    waiting::thread::until(TIMEOUT, "what the hub sent", || feed.reference(&ask(2)).is_some());
    second.show(chips(&[2, 3]));
    waiting::thread::until(TIMEOUT, "the third", || feed.reference(&ask(3)).is_some());
    drop(first);
    waiting::thread::until(TIMEOUT, "the first view to go", || said.lock().last().unwrap().shown == chips(&[2, 3]));

    let said = said.lock();
    assert!(said.iter().all(|subscribe| subscribe.subscription_id == said[0].subscription_id), "{said:?}");
    assert!(said.iter().any(|subscribe| subscribe.shown == chips(&[2, 3, 1])), "both views' together: {said:?}");
    assert_eq!(feed.reference(&ask(1)), None, "no view shows it now");
    assert_eq!(feed.reference(&ask(2)), Some(ReferenceRead::Missing));
}

/// A hub too old to say what GitHub says never says a workspace's pull request, since it keeps it on the workspace's
/// record in a form this build doesn't read.
#[test]
fn a_hub_too_old_to_say_never_says_a_pull_request() {
    let dir = tempfile::tempdir().unwrap();
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), |request| {
        let ClientRequest::GithubSubscribe(_) = request else { return Does::Nothing };
        let refused = ApiError::new(ErrorCode::Unsupported, "an older hub");
        Does::Answers(Box::new(ReplyOutcome::Error(refused)))
    });
    let feed = GithubFeed::start(&client);
    let view = feed.view();
    let workspace = protocol::fixtures::workspace("w");

    view.changed().recv_timeout(TIMEOUT).expect("told the hub is too old");

    assert!(feed.unsupported());
    assert!(!feed.said_pull_request(&workspace));
    assert_eq!(feed.pull_request(&workspace), None);
}

/// That the hub hasn't looked at a workspace's pull request is held as said, apart from the hub not having said
/// anything yet, so a bar that showed one it found can say it's looking once the hub's cache has forgotten it.
#[test]
fn that_the_hub_hasnt_looked_is_held_apart_from_it_not_having_said() {
    let dir = tempfile::tempdir().unwrap();
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), |request| {
        let ClientRequest::GithubSubscribe(subscribe) = request else { return Does::Nothing };
        let said = |found| HubFrame::Github {
            subscription_id: subscribe.subscription_id,
            update: GithubUpdate::PullRequest { workspace: "w".into(), found },
        };
        let none_found = Some(PullRequestFound { pull_request: None });
        Does::AnswersThen(Box::new(ReplyOutcome::Response(ClientResponse::Ok)), vec![said(none_found), said(None)])
    });
    let feed = GithubFeed::start(&client);
    let view = feed.view();
    let workspace = protocol::fixtures::workspace("w");
    assert!(!feed.said_pull_request(&workspace), "nothing's said before anything's shown");

    view.show(GithubShown { workspaces: vec!["w".into()], ..GithubShown::default() });
    waiting::thread::until(TIMEOUT, "the hub to say it hasn't looked", || {
        feed.said_pull_request(&workspace) && feed.pull_request(&workspace).is_none()
    });
}

/// Past the most chips a hub takes, those of the views made longest ago are left out, since the newest is likeliest to
/// be what's in front.
#[test]
fn past_the_most_chips_the_newest_views_are_kept() {
    let mut held = super::held::GithubHeld::default();
    let (changed, _) = flume::unbounded();
    held.add(1, changed.clone());
    held.add(2, changed);
    let most = u64::try_from(protocol::client::github::MOST_REFERENCES_SHOWN).unwrap();
    held.show(1, chips(&(1..=most).collect::<Vec<_>>()));
    held.show(2, chips(&[most + 1]));

    let shown = held.shown().references;

    assert_eq!(shown.len(), protocol::client::github::MOST_REFERENCES_SHOWN);
    assert_eq!(shown.first(), Some(&ask(most + 1)), "the newest view's");
    assert!(!shown.contains(&ask(most)), "the oldest view's last");
}
