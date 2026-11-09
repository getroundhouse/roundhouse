//! What has to hold however a hub's lists and notices reach a client.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, RngSeed};

use protocol::model::{AgentKind, SessionId, SessionStatus, Title, TitleSource};

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
    Session {
        id: id.clone(),
        workspace_id: "w".into(),
        agent: AgentKind::Claude,
        model: None,
        permission_mode: None,
        status: SessionStatus::Idle,
        last_seq: 0,
        created_at_ms: 0,
        updated_at_ms: 0,
        title: Title::tidied(&title.to_string()),
        closed_at_ms: None,
        reply_seq: 0,
        read_seq: 0,
        turn_started_at_ms: None,
        background_tasks: Vec::new(),
        queued: Vec::new(),
        queue_held: false,
        rev: Rev::new(rev),
        title_source: TitleSource::FirstLine,
    }
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

    fn now(&self) -> HashMap<WorkspaceId, Vec<Session>> {
        let mut now = HubRecords::default();
        now.apply_list(self.lists.last().unwrap().clone());
        now.sessions()
    }
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

        prop_assert_eq!(records.sessions(), hub.now());
    }

    /// A client that missed notices catches up with one list.
    #[test]
    fn a_list_after_the_last_change_makes_up_for_lost_notices((hub, deliveries) in some_lost_then_a_list()) {
        let mut records = HubRecords::default();

        for delivery in deliveries {
            hub.deliver(&mut records, delivery);
        }
        hub.deliver(&mut records, Delivery::List(hub.lists.len() - 1));

        prop_assert_eq!(records.sessions(), hub.now());
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

        prop_assert_eq!(records.sessions(), hub.now());
    }
}
