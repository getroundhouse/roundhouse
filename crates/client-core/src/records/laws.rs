//! What has to hold however a hub's lists and notices reach a client.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, RngSeed};

use protocol::model::{
    Account, FoundGithubLogin, GhLogin, GhLoginWorks, GithubReach, GithubRepositoryName, HubSettings, LimitWindow,
    NewWorkspaceDraft, Reach, Record, Session, SessionId, Title, UsageLimit,
};

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
        let mut deliveries = Vec::from_fn(hub.notices.len(), Delivery::Notice);
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

/// A kind of record other than a session, for what the laws above say of sessions, which holds for every kind.
struct Kind {
    name: &'static str,
    /// The record of this kind the rule is tried on, as it is at `rev`. Each `version` is a different copy of it.
    copy: fn(version: u8, rev: u64) -> Record,
    /// Another record of the kind, for a kind the hub has more than one of.
    other: Option<fn(rev: u64) -> Record>,
    /// The copy held of the record with this key, read as a view reads it, where a view does.
    shown: fn(&HubRecords, &RecordKey) -> Option<Record>,
}

fn kinds() -> [Kind; 5] {
    [
        Kind {
            name: "a hub's settings",
            copy: |version, rev| Record::HubSettings(settings(version == 2, rev)),
            other: None,
            shown: |held, _| held.snapshot().hub_settings().cloned().map(Record::HubSettings),
        },
        Kind {
            name: "what's in the new workspace dialog",
            copy: |version, rev| Record::NewWorkspaceDraft(dialog(&format!("fix the login {version}"), rev)),
            other: None,
            shown: |held, _| held.snapshot().new_workspace_draft().cloned().map(Record::NewWorkspaceDraft),
        },
        Kind {
            name: "an account",
            copy: |version, rev| Record::Account(account("ada", version * 4, rev)),
            other: Some(|rev| Record::Account(account("grace", 80, rev))),
            shown: |held, key| {
                let RecordKey::Account { id } = key else { return None };
                held.snapshot().account(id).cloned().map(Record::Account)
            },
        },
        // Held, from a hub from before Settings' Providers, though no view draws one.
        Kind {
            name: "a found GitHub login",
            copy: |version, rev| {
                let login = GhLogin::LoggedIn { username: format!("user {version}").into(), works: GhLoginWorks::Yes };
                Record::FoundGithubLogin(found("local", login, rev))
            },
            other: Some(|rev| Record::FoundGithubLogin(found("incus", GhLogin::NotInstalled, rev))),
            shown: kept,
        },
        // Held by its repository, though no view draws one yet.
        Kind {
            name: "a repository's reach",
            copy: |version, rev| {
                let cli = GithubRepositoryName::named("cli/cli");
                let username = format!("user {version}").into();
                Record::GithubReach(reach("r", Reach::Pushes { repository: cli, username }, rev))
            },
            other: Some(|rev| Record::GithubReach(reach("s", Reach::NotOnGithub, rev))),
            shown: kept,
        },
    ]
}

fn kept(held: &HubRecords, key: &RecordKey) -> Option<Record> {
    held.held.get(key).map(|kept| kept.record.clone())
}

/// A list at `rev` of these records.
fn listed(records: impl IntoIterator<Item = Record>, rev: u64) -> Records {
    let mut list = Records { rev: Rev::new(rev), ..Records::default() };
    for record in records {
        match record {
            Record::Repo(repo) => list.repos.push(repo),
            Record::Workspace(workspace) => list.workspaces.push(workspace),
            Record::Session(session) => list.sessions.push(session),
            Record::Comment(comment) => list.comments.push(comment),
            Record::HubSettings(settings) => list.hub_settings = Some(settings),
            Record::Account(account) => list.accounts.push(account),
            Record::NewWorkspaceDraft(draft) => list.new_workspace_draft = Some(draft),
            Record::GithubAccount(account) => list.github_accounts.push(account),
            Record::GithubSignIn(sign_in) => list.github_sign_ins.push(sign_in),
            Record::FoundGithubLogin(found) => list.found_github_logins.push(found),
            Record::GithubApp(app) => list.github_apps.push(app),
            Record::GithubReach(reach) => list.github_reach.push(reach),
            Record::Terminal(terminal) => list.terminals.push(terminal),
            Record::ModelList(model_list) => list.model_lists.push(model_list),
            Record::Subagent(_) => panic!("a subagent is listed with its session, not in a list of everything"),
            Record::Proposal(_) => panic!("a proposal is listed with its session, not in a list of everything"),
            Record::ReplyHeard(_) => {
                panic!("how a reply was heard is listed with its session, not in a list of everything")
            }
        }
    }
    list
}

/// Through a list, the feed, and lists from before and after a change, as a reconnect brings, each kind keeps its
/// newest copy and drops what a newer list leaves out.
#[test]
fn every_kind_of_record_keeps_its_newest_copy_as_a_session_does() {
    for Kind { name, copy, other, shown } in kinds() {
        let mut held = HubRecords::default();
        let other = other.map(|other| other(3));
        let with_other = |record: Record| [Some(record), other.clone()].into_iter().flatten();
        let key = copy(1, 2).key();
        assert_eq!(shown(&held, &key), None, "{name}, until the list has arrived");
        held.apply_list(listed([], 1));
        assert_eq!(shown(&held, &key), None, "{name}, from a hub from before it kept them");

        assert!(held.apply_list(listed(with_other(copy(1, 2)), 3)), "{name}, in a list");
        assert!(held.apply_notice(copy(2, 5).into()), "{name}, in a newer notice");
        assert!(!held.apply_notice(copy(3, 4).into()), "{name}, in an older copy that arrives late");

        assert_eq!(shown(&held, &key), Some(copy(2, 5)), "{name}");
        if let Some(other) = &other {
            assert_eq!(shown(&held, &other.key()), Some(other.clone()), "another of {name} is its own");
        }
        held.apply_list(listed(with_other(copy(1, 2)), 4));
        assert_eq!(shown(&held, &key), Some(copy(2, 5)), "{name}, after a list from before the change");
        held.apply_list(listed(with_other(copy(2, 5)), 6));
        assert_eq!(shown(&held, &key), Some(copy(2, 5)), "{name}, after a list from after it");
        held.apply_list(listed([], 9));
        let other_shown = other.as_ref().and_then(|other| shown(&held, &other.key()));
        assert_eq!((shown(&held, &key), other_shown), (None, None), "{name}, after a list without it");
    }
}

fn settings(keep_awake: bool, rev: u64) -> HubSettings {
    HubSettings {
        account_defaults: None,
        keep_awake,
        allow_asked_in_full_access: Some(false),
        github: None,
        rev: Rev::new(rev),
    }
}

fn dialog(text: &str, rev: u64) -> NewWorkspaceDraft {
    NewWorkspaceDraft { text: text.to_string(), rev: Rev::new(rev), ..NewWorkspaceDraft::default() }
}

fn account(id: &str, used_percent: u8, rev: u64) -> Account {
    let limit =
        UsageLimit { window: LimitWindow::SevenDays, model: None, used_percent, resets_at_ms: None, heard_at_ms: None };
    Account {
        email: Some(format!("{id}@example.com")),
        limits: vec![limit],
        rev: Rev::new(rev),
        ..protocol::fixtures::account(id)
    }
}

fn found(provider: &str, login: GhLogin, rev: u64) -> FoundGithubLogin {
    FoundGithubLogin { provider: provider.into(), login, rev: Rev::new(rev) }
}

fn reach(repo: &str, reach: Reach, rev: u64) -> GithubReach {
    GithubReach { repo: repo.into(), reach, answered_at_ms: None, stale: false, rev: Rev::new(rev) }
}
