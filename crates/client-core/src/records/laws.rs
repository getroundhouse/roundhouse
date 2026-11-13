//! What has to hold however a hub's lists and notices reach a client.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, RngSeed};

use protocol::model::{Account, HubSettings, LimitWindow, NewWorkspaceDraft, Session, SessionId, Title, UsageLimit};

use super::*;

/// A hub, as far as its records go: each change takes the next revision and makes a notice, and a list
/// can be read between any two changes.
#[derive(Clone, Debug)]
struct Hub {
    notices: Vec<HubNotice>,
    /// What a list would say after each number of changes.
    lists: Vec<Records>,
}

#[derive(Clone, Copy, Debug)]
enum Delivery {
    Notice(usize),
    List(usize),
}

fn session(id: &SessionId, title: u8, rev: u64) -> Session {
    Session { title: Title::tidied(&title.to_string()), rev: Rev::new(rev), ..protocol::fixtures::session(id.as_str()) }
}

impl Hub {
    /// Few records, so changes land on the same ones often. A removed record never comes back, as an
    /// ID is never reused. `revisions` off is a hub from before they existed.
    fn run(changes: &[(u8, u8)], revisions: bool) -> Hub {
        let mut held: Vec<Session> = Vec::new();
        let mut removed: Vec<SessionId> = Vec::new();
        // A hub that keeps revisions starts at 1, because 0 means one that doesn't.
        let rev_at = |count: usize| if revisions { u64::try_from(count).unwrap() + 1 } else { 0 };
        let mut hub =
            Hub { notices: Vec::new(), lists: vec![Records { rev: Rev::new(rev_at(0)), ..Records::default() }] };
        for &(which, what) in changes {
            let id = SessionId::from(format!("s{which}"));
            if removed.contains(&id) {
                continue;
            }
            let rev = rev_at(hub.notices.len() + 1);
            if what == 0 {
                if !held.iter().any(|s| s.id == id) {
                    continue;
                }
                held.retain(|s| s.id != id);
                removed.push(id.clone());
                hub.notices.push(HubNotice::Removed { record: RecordKey::Session { id }, rev: Rev::new(rev) });
            } else {
                let put = session(&id, what, rev);
                held.retain(|s| s.id != id);
                held.push(put.clone());
                hub.notices.push(HubNotice::Session(put));
            }
            hub.lists.push(Records { sessions: held.clone(), rev: Rev::new(rev), ..Records::default() });
        }
        hub
    }

    fn deliver(&self, records: &mut HubRecords, delivery: Delivery) {
        match delivery {
            Delivery::Notice(at) => records.apply_notice(self.notices[at].clone()),
            Delivery::List(at) => records.apply_list(self.lists[at].clone()),
        };
    }

    fn now(&self) -> Vec<Session> {
        let mut now = HubRecords::default();
        now.apply_list(self.lists.last().unwrap().clone());
        sessions(&now)
    }
}

/// Every session held, as a view would read them, by ID.
fn sessions(records: &HubRecords) -> Vec<Session> {
    let mut sessions: Vec<Session> = records.snapshot().every_session().cloned().collect();
    sessions.sort_by(|a, b| a.id.cmp(&b.id));
    sessions
}

fn changes() -> impl Strategy<Value = Vec<(u8, u8)>> {
    vec((0u8..4, 0u8..4), 0..24)
}

/// Every notice at least once, some twice, and lists read at any point, all in any order.
fn everything_in_any_order() -> impl Strategy<Value = (Hub, Vec<Delivery>)> {
    (changes(), vec(any::<Index>(), 0..6), vec(any::<Index>(), 0..4)).prop_flat_map(|(changes, again, lists)| {
        let hub = Hub::run(&changes, true);
        let mut deliveries: Vec<Delivery> = (0..hub.notices.len()).map(Delivery::Notice).collect();
        if !hub.notices.is_empty() {
            deliveries.extend(again.iter().map(|i| Delivery::Notice(i.index(hub.notices.len()))));
        }
        deliveries.extend(lists.iter().map(|i| Delivery::List(i.index(hub.lists.len()))));
        (Just(hub), Just(deliveries).prop_shuffle())
    })
}

/// Some notices lost, as when a client falls behind, then a list read after the last change.
fn some_lost_then_a_list() -> impl Strategy<Value = (Hub, Vec<Delivery>)> {
    (changes(), vec(any::<Index>(), 0..12), vec(any::<Index>(), 0..4)).prop_flat_map(|(changes, kept, lists)| {
        let hub = Hub::run(&changes, true);
        let mut deliveries: Vec<Delivery> = Vec::new();
        if !hub.notices.is_empty() {
            deliveries.extend(kept.iter().map(|i| Delivery::Notice(i.index(hub.notices.len()))));
        }
        deliveries.extend(lists.iter().map(|i| Delivery::List(i.index(hub.lists.len()))));
        (Just(hub), Just(deliveries).prop_shuffle())
    })
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// The reason revisions exist: a client ends with what the hub has, whatever order the feed and
    /// its lists arrive in, and however often.
    #[test]
    fn any_order_ends_with_what_the_hub_has((hub, deliveries) in everything_in_any_order()) {
        let mut records = HubRecords::default();

        for delivery in deliveries {
            hub.deliver(&mut records, delivery);
        }

        prop_assert_eq!(sessions(&records), hub.now());
    }

    /// A client that missed notices catches up with one list.
    #[test]
    fn a_list_after_the_last_change_makes_up_for_lost_notices((hub, deliveries) in some_lost_then_a_list()) {
        let mut records = HubRecords::default();

        for delivery in deliveries {
            hub.deliver(&mut records, delivery);
        }
        hub.deliver(&mut records, Delivery::List(hub.lists.len() - 1));

        prop_assert_eq!(sessions(&records), hub.now());
    }

    /// A hub without revisions: subscribe, list, then take the notices that queued since, in order,
    /// which is what one connection gives.
    #[test]
    fn without_revisions_the_order_of_one_connection_is_enough(
        changes in changes(),
        listed_at in any::<Index>(),
        subscribed_at in any::<Index>(),
    ) {
        let hub = Hub::run(&changes, false);
        let listed_at = listed_at.index(hub.lists.len());
        let mut records = HubRecords::default();

        hub.deliver(&mut records, Delivery::List(listed_at));
        for at in subscribed_at.index(listed_at + 1)..hub.notices.len() {
            hub.deliver(&mut records, Delivery::Notice(at));
        }

        prop_assert_eq!(sessions(&records), hub.now());
    }
}

fn settings(keep_awake: bool, rev: u64) -> HubSettings {
    HubSettings { keep_awake, allow_asked_in_full_access: Some(false), rev: Rev::new(rev) }
}

#[test]
fn a_hubs_settings_are_a_record_like_the_rest() {
    let mut held = HubRecords::default();
    assert_eq!(held.snapshot().hub_settings().cloned(), None);

    assert!(held.apply_list(Records { hub_settings: Some(settings(true, 2)), rev: Rev::new(2), ..Records::default() }));
    assert!(held.apply_notice(HubNotice::HubSettings(settings(false, 5))));
    assert!(!held.apply_notice(HubNotice::HubSettings(settings(true, 4))), "an older copy that arrives late");

    assert_eq!(held.snapshot().hub_settings().cloned(), Some(settings(false, 5)));
    // A list from before the change, and one from after it, as a reconnect brings.
    held.apply_list(Records { hub_settings: Some(settings(true, 2)), rev: Rev::new(3), ..Records::default() });
    assert_eq!(held.snapshot().hub_settings().cloned(), Some(settings(false, 5)));
    held.apply_list(Records { hub_settings: Some(settings(false, 5)), rev: Rev::new(6), ..Records::default() });
    assert_eq!(held.snapshot().hub_settings().cloned(), Some(settings(false, 5)));
}

#[test]
fn a_hub_from_before_it_kept_settings_has_none() {
    let mut held = HubRecords::default();

    held.apply_list(Records { rev: Rev::new(3), ..Records::default() });

    assert_eq!(held.snapshot().hub_settings().cloned(), None);
}

fn dialog(text: &str, rev: u64) -> NewWorkspaceDraft {
    NewWorkspaceDraft { text: text.to_string(), rev: Rev::new(rev), ..NewWorkspaceDraft::default() }
}

#[test]
fn whats_in_the_new_workspace_dialog_is_a_record_like_the_rest() {
    let mut held = HubRecords::default();
    assert_eq!(
        held.snapshot().new_workspace_draft().cloned(),
        None,
        "from a hub from before it kept one, and until the list has arrived"
    );

    assert!(held.apply_list(Records {
        new_workspace_draft: Some(dialog("", 2)),
        rev: Rev::new(2),
        ..Records::default()
    }));
    assert!(held.apply_notice(HubNotice::NewWorkspaceDraft(dialog("fix the login", 5))));
    assert!(!held.apply_notice(HubNotice::NewWorkspaceDraft(dialog("fix", 4))), "an older copy that arrives late");

    assert_eq!(held.snapshot().new_workspace_draft().cloned(), Some(dialog("fix the login", 5)));
    held.apply_list(Records { new_workspace_draft: Some(dialog("", 2)), rev: Rev::new(3), ..Records::default() });
    assert_eq!(
        held.snapshot().new_workspace_draft().cloned(),
        Some(dialog("fix the login", 5)),
        "a list from before the change"
    );
}

fn account(id: &str, used_percent: u8, rev: u64) -> Account {
    let limit =
        UsageLimit { window: LimitWindow::SevenDays, model: None, used_percent, resets_at_ms: None, heard_at_ms: None };
    Account {
        id: id.into(),
        model_provider: None,
        plan: None,
        login: None,
        email: Some(format!("{id}@example.com")),
        name: None,
        login_agent: None,
        limits: vec![limit],
        held_login: None,
        rev: Rev::new(rev),
    }
}

#[test]
fn each_account_is_a_record_like_the_rest() {
    let mut held = HubRecords::default();
    assert_eq!(held.snapshot().account(&"ada".into()).cloned(), None);

    let listed = vec![account("ada", 4, 2), account("grace", 80, 3)];
    assert!(held.apply_list(Records { accounts: listed, rev: Rev::new(3), ..Records::default() }));
    assert!(held.apply_notice(HubNotice::Account(account("ada", 9, 5))));
    assert!(!held.apply_notice(HubNotice::Account(account("ada", 6, 4))), "an older copy that arrives late");

    assert_eq!(held.snapshot().account(&"ada".into()).cloned(), Some(account("ada", 9, 5)));
    assert_eq!(
        held.snapshot().account(&"grace".into()).cloned(),
        Some(account("grace", 80, 3)),
        "another account is its own"
    );
    // A list from before the change, as a reconnect can bring.
    held.apply_list(Records { accounts: vec![account("ada", 4, 2)], rev: Rev::new(4), ..Records::default() });
    assert_eq!(held.snapshot().account(&"ada".into()).cloned(), Some(account("ada", 9, 5)));
}
