//! Against a stand-in hub on a Unix socket.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::github::GithubUpdate;
use protocol::client::listed::{Listed, ReferenceObservations};
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{ApiError, ClientRequest, ErrorCode, HubFrame, ReplyOutcome};
use protocol::model::{
    GithubLoginKind, GithubRepositoryName, IssueNumber, Level, Observation, ObservationKey, Observed, PullRequestFound,
    ReferenceAsk, ReferenceRead, ReferenceSaid, RepositoryUnseen, Rev,
};

use super::super::held::GithubHeld;
use super::super::source::GithubSource;
use crate::client::follow::{Following, Source as _};
use crate::records::HubRecords;
use crate::stand_in_hub::{self, Does, refusing_every_start, starts, subscribed};
use crate::{Client, RecordsFeed, ShownOfGithub, SubscriptionItem};

const TIMEOUT: Duration = Duration::from_secs(10);

fn ask(n: u64) -> ReferenceAsk {
    ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(n).unwrap() }
}

fn chips(numbers: &[u64]) -> ShownOfGithub {
    ShownOfGithub { references: numbers.iter().map(|n| ask(*n)).collect(), ..ShownOfGithub::default() }
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

/// A hub on `socket` that keeps what it's asked to follow, lists none of its records, and answers each issue or pull
/// request it's asked to follow with it missing, as a hub that has read them sends.
fn hub_that_says_missing(socket: &std::path::Path) -> (crate::Client, Asked) {
    let asked = Asked::default();
    let heard = asked.clone();
    let client = stand_in_hub::connect(socket, move |request| {
        if request == ClientRequest::AgentList {
            return Does::Answers(Box::new(ReplyOutcome::Error(agents_unknown())));
        }
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
            if *follow == Follow::Hub {
                return vec![HubFrame::Listed { subscription_id, listed: Listed::Hub(Box::default()) }];
            }
            let Follow::Reference { reference } = follow else { return Vec::new() };
            let update = GithubUpdate::Reference { reference: reference.clone(), read: ReferenceRead::Missing };
            vec![HubFrame::Github { subscription_id, update }]
        })
    });
    (client, asked)
}

/// What a hub from before it described its agents answers when asked for them.
fn agents_unknown() -> ApiError {
    ApiError::new(ErrorCode::Unsupported, "a hub from before it described its agents")
}

fn chip(n: u64) -> Level {
    Level::Reference { reference: ask(n) }
}

/// The chips every view shows are followed together, under one subscription, which each change to what's shown changes
/// in one request, and what the hub sends of them reaches the views that show them. What a view showed is let go of
/// once it's gone. A workspace a view shows the pull request of is followed only at the hub's level.
#[test]
fn what_views_show_is_followed_together_and_what_the_hub_sends_reaches_them() {
    let dir = tempfile::tempdir().unwrap();
    let (client, asked) = hub_that_says_missing(&dir.path().join("hub.sock"));
    let records = RecordsFeed::start(&client);
    let feed = records.github();
    let (first, second) = (feed.view(), feed.view());

    first.show(chips(&[1, 2]));
    first.changed().recv_timeout(TIMEOUT).expect("the first view hears of what it shows");
    waiting::thread::until(TIMEOUT, "what the hub sent", || feed.reference(&ask(2)).is_some());
    second.show(ShownOfGithub { workspaces: vec!["w".into()], ..chips(&[2, 3]) });
    waiting::thread::until(TIMEOUT, "the third", || feed.reference(&ask(3)).is_some());
    let both = asked.follows();
    drop(first);
    waiting::thread::until(TIMEOUT, "the first view to go", || !asked.follows().contains(&chip(1)));

    assert_eq!(both, [Level::Hub, chip(1), chip(2), chip(3)], "both views' together, each once");
    assert_eq!(asked.follows(), [Level::Hub, chip(2), chip(3)]);
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
            return Does::Answers(Box::new(ReplyOutcome::Error(agents_unknown())));
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
    let nothing_followed = GithubHeld::default();
    let none = |_: &ObservationKey| None;
    assert!(!nothing_followed.said_pull_request(&workspace.id, &none), "nothing's said before the hub is followed");

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
    let mut held = GithubHeld::default();
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

/// A start the hub refused isn't asked for again each time a view says what it shows, as views do whenever what they
/// draw from changes, only once what's shown changes.
#[test]
fn a_refused_start_is_asked_for_again_only_once_what_s_shown_changes() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let heard = stand_in_hub::listen(&socket, |request| {
        Does::Answers(Box::new(refusing_every_start(&request, ErrorCode::NotFound, "a stand-in has no issues")))
    });
    let client = Client::connect(&socket, "test").unwrap();
    let records = Following::unstarted(HubRecords::default());
    let (held, mut source) = (Mutex::new(GithubHeld::default()), GithubSource::new(records.reach()));
    held.lock().add(1, flume::unbounded().0);
    let _subscription = source.resume(&client, &held).unwrap();
    let mut show = |shown| {
        held.lock().show(1, shown);
        source.asked(Some(&client), &held, &|| {}).unwrap();
    };

    for _ in 0..3 {
        show(chips(&[1]));
    }
    let while_the_same = starts(&heard);
    show(chips(&[1, 2]));

    assert_eq!(while_the_same, [1], "asked for once, and refused");
    assert_eq!(starts(&heard), [2], "both, once what's shown changed");
}

/// What a chip's level sends that was already on its way as the chip was let go of, such as its list after falling
/// behind, is dropped, so nothing brings back what letting go of it let go of.
#[test]
fn what_s_sent_of_a_chip_no_longer_followed_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let (client, _) = hub_that_says_missing(&dir.path().join("hub.sock"));
    let records = Following::unstarted(HubRecords::default());
    let (held, mut source) = (Mutex::new(GithubHeld::default()), GithubSource::new(records.reach()));
    held.lock().add(1, flume::unbounded().0);
    held.lock().show(1, chips(&[1]));
    let _subscription = source.resume(&client, &held).unwrap();
    let listed = || {
        let said = Observed::Current { value: ReferenceSaid::Missing };
        let observation = Observation::Reference { reference: ask(1), said, rev: Rev::new(1) };
        let listed = ReferenceObservations { reference: ask(1), observations: vec![observation], rev: Rev::new(1) };
        SubscriptionItem::Listed(Box::new(Listed::Reference(listed)))
    };
    let held_of_it = || {
        let key = ObservationKey::Reference { reference: ask(1) };
        records.reach().read(|records| records.snapshot().github_observation(&key).is_some()).unwrap()
    };
    source.apply(&client, &held, listed()).unwrap();
    assert!(held_of_it(), "followed");

    held.lock().show(1, ShownOfGithub::default());
    source.asked(Some(&client), &held, &|| {}).unwrap();
    source.apply(&client, &held, listed()).unwrap();

    assert!(!held_of_it(), "let go of, and not listed again by what came after");
}
