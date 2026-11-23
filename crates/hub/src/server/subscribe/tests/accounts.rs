//! Who an agent's account is, which a client of version 4 reads on the account's record, and a client of the version
//! that reads every observation reads as an observation, on a hub whose database is from before it left the record.

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{EVERY_OBSERVATION_FROM, HubFrame, HubNotice};
use protocol::model::{Account, AgentAccount, DiffComment, DiffSide, Observation, Observed, Rev};

use super::given::{ok, start};
use crate::hub::tests::hub_in_again;
use crate::server::tests::Connection;
use crate::store::BEFORE_ACCOUNT_IDENTITY_OBSERVED;
use crate::store::older::store_from_before;
use crate::store::tests::idle_session;

/// Ada's account, as a worker says it, on the plan `plan`.
fn ada(plan: &str) -> AgentAccount {
    AgentAccount {
        id: Some("claude-ada".into()),
        logged_in: true,
        model_provider: Some("Anthropic".into()),
        plan: Some(plan.into()),
        login: Some("Claude account".into()),
        email: Some("ada@example.com".into()),
    }
}

/// The account's record, as the hub's level lists it to `client`.
fn listed_account(client: &Connection) -> (Account, Vec<Observation>) {
    let records = client.sent.iter().find_map(|frame| {
        let HubFrame::Listed { listed: Listed::Hub(records), .. } = frame else { return None };
        Some(records)
    });
    let records = records.expect("the hub's level listed");
    let account = records.accounts.iter().find(|account| account.id.as_str() == "claude-ada").cloned();
    (account.expect("the account listed"), records.observations.clone())
}

/// Each copy of the account's record `client` was sent.
fn accounts_sent(client: &Connection) -> Vec<Account> {
    let copies = client.sent.iter().filter_map(|frame| {
        let HubFrame::Notice { notice: HubNotice::Account(account), .. } = frame else { return None };
        Some(account.clone())
    });
    copies.collect()
}

/// Whether `sent` tells who the account is, as it says on `plan`.
fn told_who_it_is(sent: &[HubFrame], plan: &str) -> bool {
    sent.iter().any(|frame| {
        let HubFrame::Notice { notice: HubNotice::Observation(Observation::AccountIdentity { identity, .. }), .. } =
            frame
        else {
            return false;
        };
        matches!(identity, Observed::AsOf { value, .. } if value.plan.as_deref() == Some(plan))
    })
}

/// A hub on a database from before who an account is left it lists the account to a client of version 4 with the
/// four fields empty, and to one of the version that reads every observation with no observation of it. The worker's
/// next read fills it: the first client is sent the record again, filled, under a newer revision, and so again as the
/// plan changes, and the second is sent the observation, and no copy of the record that moves only those fields.
#[tokio::test]
async fn who_an_account_is_reaches_each_client_in_the_form_its_version_reads() {
    let dir = tempfile::tempdir().unwrap();
    let rows = "INSERT INTO accounts (id, model_provider, plan, login, email)
                    VALUES ('claude-ada', 'Anthropic', 'Pro', 'Claude account', 'ada@example.com');";
    let store = store_from_before(dir.path(), BEFORE_ACCOUNT_IDENTITY_OBSERVED, rows);
    store.insert_session(&idle_session(crate::store::now_ms()), None).unwrap();
    drop(store);
    let hub = hub_in_again(dir.path());
    let mut before = Connection::greeted(&hub, 4).await;
    let mut observing = Connection::speaking(&hub, EVERY_OBSERVATION_FROM);
    for client in [&mut before, &mut observing] {
        ok(client, 1, Subscribe { stop: Vec::new(), start: vec![start(1, Follow::Hub)] }).await;
        client.until("the list", |sent| sent.iter().any(|frame| matches!(frame, HubFrame::Listed { .. }))).await;
    }
    let (listed, observations) = listed_account(&before);
    assert_eq!((&listed.email, &listed.plan, &listed.model_provider, &listed.login), (&None, &None, &None, &None));
    assert!(observations.is_empty(), "{observations:?}");
    assert!(listed_account(&observing).1.is_empty(), "not known");

    hub.launched_under(&"s".into(), ada("Max"), 1_000).await.unwrap();
    hub.launched_under(&"s".into(), ada("Team"), 2_000).await.unwrap();

    let marker = HubNotice::Comment(DiffComment {
        id: "c".into(),
        workspace_id: "w".into(),
        path: "a.rs".into(),
        side: DiffSide::New,
        line: 1,
        text: "after".into(),
        created_at_ms: 1,
        // A copy with no revision is always sent.
        rev: Rev::default(),
    });
    hub.notify(marker.clone());

    for client in [&mut before, &mut observing] {
        let at_the_marker = |sent: &[HubFrame]| {
            sent.iter().any(|frame| matches!(frame, HubFrame::Notice { notice, .. } if *notice == marker))
        };
        client.until("the marker", at_the_marker).await;
    }
    let copies = accounts_sent(&before);
    let plans: Vec<Option<&str>> = copies.iter().map(|account| account.plan.as_deref()).collect();
    assert!(plans.ends_with(&[Some("Max"), Some("Team")]), "{plans:?}");
    let revs: Vec<Rev> = copies.iter().map(|account| account.rev).collect();
    assert!(revs.is_sorted() && revs.iter().all(|rev| *rev > listed.rev), "{revs:?} after {:?}", listed.rev);
    assert!(!told_who_it_is(&before.sent, "Team"), "no observation to a client of version 4");
    assert!(told_who_it_is(&observing.sent, "Max") && told_who_it_is(&observing.sent, "Team"));
    let filled = accounts_sent(&observing).into_iter().filter(|account| account.plan.is_some());
    assert_eq!(filled.count(), 0, "no copy that moves only who it is");
}
