//! Who an agent's account is, which the hub keeps as an observation.

use protocol::model::{AccountId, AccountIdentity, AgentAccount, Observation, Observed};
use rusqlite::Connection;

use super::on_a_machine::the_week;
use crate::hub::Hub;
use crate::hub::tests::{hub_in, hub_in_again};
use crate::store::BEFORE_ACCOUNT_IDENTITY_OBSERVED;
use crate::store::columns::OneValue;
use crate::store::older::store_from_before;

/// The account a worker says the machine's login is, as Ada's on the Max plan.
pub(crate) fn ada(plan: &str) -> AgentAccount {
    AgentAccount {
        id: Some("claude-ada".into()),
        logged_in: true,
        model_provider: Some("Anthropic".into()),
        plan: Some(plan.into()),
        login: Some("Claude account".into()),
        email: Some("ada@example.com".into()),
    }
}

/// Who `ada` says the account is.
fn ada_is(plan: &str) -> AccountIdentity {
    AccountIdentity {
        model_provider: Some("Anthropic".into()),
        plan: Some(plan.into()),
        login: Some("Claude account".into()),
        email: Some("ada@example.com".into()),
    }
}

/// What `hub` lists of who `account` is, and its record.
async fn listed(hub: &Hub, account: &str) -> (Option<Observed<AccountIdentity>>, Option<protocol::model::Account>) {
    let records = hub.records_list().await.unwrap();
    let observed = records.observations.into_iter().find_map(|observation| {
        let Observation::AccountIdentity { account: of, identity, .. } = observation else { return None };
        (of.as_str() == account).then_some(identity)
    });
    (observed, records.accounts.into_iter().find(|record| record.id.as_str() == account))
}

/// A hub that starts again has who an account is as of the worker's read.
#[tokio::test]
async fn who_an_account_is_is_kept_across_a_restart_as_of_its_read() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.launched_under(&"s".into(), ada("Max"), 1_000).await.unwrap();
    hub.observations_written().await;
    drop(hub);

    let again = hub_in_again(dir.path());

    let (observed, record) = listed(&again, "claude-ada").await;
    assert_eq!(observed, Some(Observed::AsOf { value: ada_is("Max"), as_of_ms: 1_000 }));
    assert!(record.is_some());
}

/// A database from before who an account is left it opens with the columns gone and the record as it was but for
/// them, and who the account is not known, as nothing is carried out of the database, until a worker next says.
#[tokio::test]
async fn an_account_from_before_opens_with_who_it_is_not_known() {
    let dir = tempfile::tempdir().unwrap();
    let rows = "INSERT INTO accounts (id, model_provider, plan, login, email, name)
                    VALUES ('claude-ada', 'Anthropic', 'Max', 'Claude account', 'ada@example.com', 'Work');";
    drop(store_from_before(dir.path(), BEFORE_ACCOUNT_IDENTITY_OBSERVED, rows));

    let hub = hub_in_again(dir.path());

    let db = Connection::open(dir.path().join("hub.db")).unwrap();
    let columns: Vec<String> = db
        .prepare("SELECT name FROM pragma_table_info('accounts')")
        .unwrap()
        .query_map([], OneValue::read)
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for gone in ["model_provider", "plan", "login", "email"] {
        assert!(!columns.iter().any(|column| column == gone), "{gone} in {columns:?}");
    }
    let (observed, record) = listed(&hub, "claude-ada").await;
    let record = record.expect("the record kept");
    assert_eq!(observed, None, "not known");
    assert_eq!(record.name.as_deref(), Some("Work"));
}

/// Who an account is and what's left of its limits go with the account: a hub that starts on a database without the
/// account keeps neither.
#[tokio::test]
async fn what_s_observed_of_an_account_is_forgotten_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let account = AccountId::from("claude-ada");
    hub.store.account_made(&account).unwrap();
    hub.observation_read::<crate::hub::observations::AccountIdentity>(account.clone(), ada_is("Max"), 1_000)
        .await
        .unwrap();
    hub.hear_limits(account, vec![the_week()], false, 1_000).await;
    hub.observations_written().await;
    drop(hub);
    let db = Connection::open(dir.path().join("hub.db")).unwrap();
    db.execute("DELETE FROM accounts WHERE id = 'claude-ada'", []).unwrap();

    let again = hub_in_again(dir.path());

    assert_eq!(listed(&again, "claude-ada").await, (None, None));
    let kept = again.records_list().await.unwrap().observations;
    assert!(!kept.iter().any(|observation| matches!(observation, Observation::AccountLimits { .. })), "{kept:?}");
}
