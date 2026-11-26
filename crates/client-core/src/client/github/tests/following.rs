//! Against a stand-in hub on a Unix socket.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::listed::{Listed, ReferenceObservations};
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{ApiError, ClientRequest, ErrorCode, HubFrame, ReplyOutcome};
use protocol::model::{
    GithubRepositoryName, IssueNumber, Level, Observation, ObservationKey, Observed, ReferenceAsk, ReferenceSaid, Rev,
};

use super::super::held::GithubHeld;
use super::super::source::GithubSource;
use crate::client::follow::{Following, Source as _};
use crate::records::HubRecords;
use crate::stand_in_hub::{self, Does, refusing_every_start, starts, subscribed};
use crate::{Client, RecordsFeed, SubscriptionItem};

const TIMEOUT: Duration = Duration::from_secs(10);

fn ask(n: u64) -> ReferenceAsk {
    ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(n).unwrap() }
}

fn chips(numbers: &[u64]) -> Vec<ReferenceAsk> {
    numbers.iter().map(|n| ask(*n)).collect()
}

/// What the copy of the hub's records holds of what the hub said of `ask`.
fn said(records: &RecordsFeed, ask: &ReferenceAsk) -> Observed<ReferenceSaid> {
    records.snapshot().reference_said(ask).clone()
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
            let said = Observed::Current { value: ReferenceSaid::Missing };
            let missing = Observation::Reference { reference: reference.clone(), said, rev: Rev::new(2) };
            let listed =
                ReferenceObservations { reference: reference.clone(), observations: vec![missing], rev: Rev::new(2) };
            vec![HubFrame::Listed { subscription_id, listed: Listed::Reference(listed) }]
        })
    });
    (client, asked)
}

/// A refusal to describe the hub's agents, which a client takes as none described.
fn agents_unknown() -> ApiError {
    ApiError::new(ErrorCode::Unsupported, "refused")
}

fn chip(n: u64) -> Level {
    Level::Reference { reference: ask(n) }
}

/// The chips every view shows are followed together, under one subscription, which each change to what's shown changes
/// in one request, and what the hub sends of them goes into the copy of the hub's records, whose readers are told. What
/// a view showed is let go of once it's gone.
#[test]
fn what_views_show_is_followed_together_and_what_the_hub_sends_reaches_them() {
    let dir = tempfile::tempdir().unwrap();
    let (client, asked) = hub_that_says_missing(&dir.path().join("hub.sock"));
    let records = RecordsFeed::start(&client);
    waiting::thread::until(TIMEOUT, "the hub's records", || records.connected());
    let feed = records.github();
    let (first, second) = (feed.view(), feed.view());
    while records.changed().try_recv().is_ok() {}

    first.show(chips(&[1, 2]));
    records.changed().recv_timeout(TIMEOUT).expect("the records' readers hear of what the chips' level sent");
    waiting::thread::until(TIMEOUT, "what the hub sent", || said(&records, &ask(2)) != Observed::NotKnown);
    second.show(chips(&[2, 3]));
    waiting::thread::until(TIMEOUT, "the third", || said(&records, &ask(3)) != Observed::NotKnown);
    let both = asked.follows();
    drop(first);
    waiting::thread::until(TIMEOUT, "the first view to go", || !asked.follows().contains(&chip(1)));
    // The copy lets go of it once the hub has answered, which is after the hub heard.
    waiting::thread::until(TIMEOUT, "what no view shows now to be let go of", || {
        said(&records, &ask(1)) == Observed::NotKnown
    });

    assert_eq!(both, [Level::Hub, chip(1), chip(2), chip(3)], "both views' together, each once");
    assert_eq!(asked.follows(), [Level::Hub, chip(2), chip(3)]);
    let last = asked.subscribes.lock().last().cloned().unwrap();
    assert_eq!((last.stop.len(), last.start.len()), (1, 0), "one request stops what's no longer shown: {last:?}");
    assert_eq!(said(&records, &ask(2)), Observed::Current { value: ReferenceSaid::Missing });
}

/// Past the most chips a hub takes, those of the views made longest ago are left out, since the newest is likeliest to
/// be what's in front.
#[test]
fn past_the_most_chips_the_newest_views_are_kept() {
    let mut held = GithubHeld::default();
    held.add(1);
    held.add(2);
    let most = u64::try_from(protocol::client::github::MOST_REFERENCES_SHOWN).unwrap();
    held.show(1, chips(&(1..=most).collect::<Vec<_>>()));
    held.show(2, chips(&[most + 1]));

    let shown = held.shown();

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
    held.lock().add(1);
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
    held.lock().add(1);
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
        records.reach().read(|records| records.snapshot().observation(&key).is_some()).unwrap()
    };
    source.apply(&client, &held, listed()).unwrap();
    assert!(held_of_it(), "followed");

    held.lock().show(1, Vec::new());
    source.asked(Some(&client), &held, &|| {}).unwrap();
    source.apply(&client, &held, listed()).unwrap();

    assert!(!held_of_it(), "let go of, and not listed again by what came after");
}
