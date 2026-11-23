//! Against a stand-in hub on a Unix socket.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::github::{GithubShown, GithubUpdate};
use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{ApiError, ClientRequest, ErrorCode, HubFrame, ReplyOutcome};
use protocol::model::{
    GithubLoginKind, GithubRepositoryName, IssueNumber, Level, PullRequestFound, ReferenceAsk, ReferenceRead,
    RepositoryUnseen,
};

use super::GithubFeed;
use crate::RecordsFeed;
use crate::stand_in_hub::{self, Does, subscribed};

const TIMEOUT: Duration = Duration::from_secs(10);

fn ask(n: u64) -> ReferenceAsk {
    ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(n).unwrap() }
}

fn chips(numbers: &[u64]) -> GithubShown {
    GithubShown { references: numbers.iter().map(|n| ask(*n)).collect(), ..GithubShown::default() }
}

/// What a stand-in hub has been asked to follow, by subscription ID, and each `Subscribe` it was sent, in order.
#[derive(Clone, Default)]
struct Asked {
    followed: Arc<Mutex<BTreeMap<u64, Level>>>,
    subscribes: Arc<Mutex<Vec<Subscribe>>>,
}

impl Asked {
    fn follows(&self) -> Vec<Level> {
        let mut levels: Vec<Level> = self.followed.lock().values().cloned().collect();
        levels.sort_by_key(|level| format!("{level:?}"));
        levels
    }
}

/// A hub on `socket` that keeps what it's asked to follow, and answers each issue or pull request it's asked to follow
/// with it missing, as a hub that has read them sends.
fn hub_that_says_missing(socket: &std::path::Path) -> (crate::Client, Asked) {
    let asked = Asked::default();
    let heard = asked.clone();
    let client = stand_in_hub::connect(socket, move |request| {
        let ClientRequest::Subscribe(subscribe) = request else { return Does::Nothing };
        let mut followed = heard.followed.lock();
        for id in &subscribe.stop {
            followed.remove(id);
        }
        for start in &subscribe.start {
            followed.insert(start.subscription_id, start.follow.level());
        }
        heard.subscribes.lock().push(subscribe.clone());
        subscribed(&subscribe, |subscription_id, follow| {
            let Follow::Reference { reference } = follow else { return Vec::new() };
            let update = GithubUpdate::Reference { reference: reference.clone(), read: ReferenceRead::Missing };
            vec![HubFrame::Github { subscription_id, update }]
        })
    });
    (client, asked)
}

fn chip(n: u64) -> Level {
    Level::Reference { reference: ask(n) }
}

/// What every view shows is followed together, under one subscription, which each change to what's shown changes in
/// one request, and what the hub sends of it reaches the views that show it. What a view showed is let go of once it's
/// gone. A workspace a view shows the pull request of in full is followed at its own level.
#[test]
fn what_views_show_is_followed_together_and_what_the_hub_sends_reaches_them() {
    let dir = tempfile::tempdir().unwrap();
    let (client, asked) = hub_that_says_missing(&dir.path().join("hub.sock"));
    let feed = GithubFeed::start(&client);
    let (first, second) = (feed.view(), feed.view());

    first.show(chips(&[1, 2]));
    first.changed().recv_timeout(TIMEOUT).expect("the first view hears of what it shows");
    waiting::thread::until(TIMEOUT, "what the hub sent", || feed.reference(&ask(2)).is_some());
    second.show(GithubShown { closely: vec!["w".into()], ..chips(&[2, 3]) });
    waiting::thread::until(TIMEOUT, "the third", || feed.reference(&ask(3)).is_some());
    let both = asked.follows();
    drop(first);
    waiting::thread::until(TIMEOUT, "the first view to go", || !asked.follows().contains(&chip(1)));

    let workspace = Level::Workspace { id: "w".into() };
    assert_eq!(both, [chip(1), chip(2), chip(3), workspace.clone()], "both views' together, each once");
    assert_eq!(asked.follows(), [chip(2), chip(3), workspace]);
    let last = asked.subscribes.lock().last().cloned().unwrap();
    assert_eq!((last.stop.len(), last.start.len()), (1, 0), "one request stops what's no longer shown: {last:?}");
    assert_eq!(feed.reference(&ask(1)), None, "no view shows it now");
    assert_eq!(feed.reference(&ask(2)), Some(ReferenceRead::Missing));
}

/// Every workspace's own pull request comes from following the hub, whatever views show. That the hub hasn't looked
/// at one is held as said, apart from the hub not having said anything yet, so a bar that showed one it found can say
/// it's looking once the hub's cache has forgotten it. And GitHub showing the default no repository to look in is held
/// beside it.
#[test]
fn every_workspaces_pull_request_comes_from_following_the_hub_and_that_it_hasnt_looked_is_held_apart() {
    let dir = tempfile::tempdir().unwrap();
    let unseen = RepositoryUnseen {
        repository: GithubRepositoryName::named("o/r"),
        username: "ada".into(),
        login: GithubLoginKind::Token,
        installation: None,
        said: "Could not resolve to a Repository with the name 'o/r'.".into(),
        approve_at: None,
    };
    let told = unseen.clone();
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), move |request| {
        if request == ClientRequest::AgentList {
            let unknown = ApiError::new(ErrorCode::Unsupported, "a hub from before it described its agents");
            return Does::Answers(Box::new(ReplyOutcome::Error(unknown)));
        }
        let ClientRequest::Subscribe(subscribe) = request else { return Does::Nothing };
        subscribed(&subscribe, |subscription_id, follow| {
            if *follow != Follow::Hub {
                return Vec::new();
            }
            let said = |found, unseen| HubFrame::Github {
                subscription_id,
                update: GithubUpdate::PullRequest { workspace: "w".into(), found, unseen },
            };
            let listed = HubFrame::Listed { subscription_id, listed: Listed::Hub(Box::default()) };
            let none_found = Some(PullRequestFound { pull_request: None });
            vec![listed, said(none_found, None), said(None, Some(told.clone()))]
        })
    });
    let workspace = protocol::fixtures::workspace("w");
    let nothing_followed = GithubFeed::start(&client);
    assert!(!nothing_followed.said_pull_request(&workspace), "nothing's said before the hub is followed");

    let records = RecordsFeed::start(&client);
    let feed = records.github();

    waiting::thread::until(TIMEOUT, "the hub to say it hasn't looked, and can't see where to", || {
        feed.said_pull_request(&workspace)
            && feed.pull_request(&workspace).is_none()
            && feed.unseen(&workspace).is_some()
    });
    assert_eq!(feed.unseen(&workspace), Some(unseen));
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
