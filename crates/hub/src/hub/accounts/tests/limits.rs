//! What's left of an account's limits, which the hub keeps as an observation.

use protocol::model::{Observation, Observed, UsageLimit};
use rusqlite::Connection;

use super::on_a_machine::the_week as week;
use crate::hub::Hub;
use crate::hub::observations::AccountLimits;
use crate::hub::tests::{hub_in, hub_in_again};
use crate::store::BEFORE_ACCOUNT_LIMITS_OBSERVED;
use crate::store::columns::OneValue;
use crate::store::older::store_from_before;

/// The account's week, a limit an agent under it said.
fn the_week() -> Vec<UsageLimit> {
    vec![week()]
}

/// `limits` as heard at `heard_at_ms`, as the hub keeps them.
fn heard_at(limits: Vec<UsageLimit>, heard_at_ms: i64) -> Vec<UsageLimit> {
    limits.into_iter().map(|limit| UsageLimit { heard_at_ms: Some(heard_at_ms), ..limit }).collect()
}

/// What `hub` lists of what's left of `account`'s limits.
async fn listed(hub: &Hub, account: &str) -> Option<Observed<Vec<UsageLimit>>> {
    let records = hub.records_list().await.unwrap();
    records.observations.into_iter().find_map(|observation| {
        let Observation::AccountLimits { account: of, limits, .. } = observation else { return None };
        (of.as_str() == account).then_some(limits)
    })
}

/// A hub that starts again has what's left of an account's limits as of the newest hearing, each with when it was
/// heard.
#[tokio::test]
async fn an_account_s_limits_are_kept_across_a_restart_as_of_the_newest_hearing() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.hear_limits("ada".into(), the_week(), false, 1_000).await;
    hub.observations_written().await;
    drop(hub);

    let again = hub_in_again(dir.path());

    let observed = listed(&again, "ada").await;
    assert_eq!(observed, Some(Observed::AsOf { value: heard_at(the_week(), 1_000), as_of_ms: 1_000 }));
}

/// Hearing the same again tells no client and moves no revision, since only when it was heard is new, which the hub
/// keeps all the same, in memory and in the file, so a hearing that lands late is taken as no newer.
#[tokio::test]
async fn hearing_the_same_again_tells_nobody_and_is_kept_as_heard_later() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.hear_limits("ada".into(), the_week(), false, 1_000).await;
    let mut feed = hub.subscribe_feed();

    hub.hear_limits("ada".into(), the_week(), false, 2_000).await;

    assert!(feed.try_recv().is_err(), "nothing told");
    assert_eq!(hub.observations.value_of::<AccountLimits>(&"ada".into()), Some(heard_at(the_week(), 2_000)));
    hub.observations_written().await;
    drop((hub, feed));
    let again = hub_in_again(dir.path());
    let observed = listed(&again, "ada").await;
    let Some(Observed::AsOf { value, .. }) = observed else { panic!("{observed:?}") };
    assert_eq!(value, heard_at(the_week(), 2_000), "the file keeps when it was heard");
}

/// A database from before what's left of an account's limits left it opens with the table gone and the account's
/// record as it was, and its limits not known, as nothing is carried out of the database, until an agent under it next
/// says them.
#[tokio::test]
async fn an_account_from_before_opens_with_its_limits_not_known() {
    let dir = tempfile::tempdir().unwrap();
    let rows = "INSERT INTO accounts (id, name) VALUES ('ada', 'Work');
                INSERT INTO account_limits (account_id, window, used_percent, heard_at_ms)
                    VALUES ('ada', 'seven_days', 100, 1000);";
    drop(store_from_before(dir.path(), BEFORE_ACCOUNT_LIMITS_OBSERVED, rows));

    let hub = hub_in_again(dir.path());

    let db = Connection::open(dir.path().join("hub.db")).unwrap();
    let table: i64 = db
        .query_row("SELECT count(*) FROM sqlite_schema WHERE name LIKE 'account_limits%'", [], OneValue::read)
        .unwrap();
    assert_eq!(table, 0, "the table and its triggers gone");
    assert_eq!(listed(&hub, "ada").await, None, "not known");
    let record = hub.store.account(&"ada".into()).unwrap().unwrap();
    assert_eq!(record.name.as_deref(), Some("Work"));
    let used_up = UsageLimit { used_percent: 100, ..week() };
    hub.hear_limits("ada".into(), vec![used_up.clone()], false, 2_000).await;
    let said_again = Observed::AsOf { value: heard_at(vec![used_up], 2_000), as_of_ms: 2_000 };
    assert_eq!(listed(&hub, "ada").await, Some(said_again));
}
