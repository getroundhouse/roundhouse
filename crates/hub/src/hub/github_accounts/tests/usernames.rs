//! What GitHub calls a GitHub account's user, and what its profile says, which the hub keeps as observations, and fills
//! the account's record with for a client of a version from before: across a restart, from a database from before,
//! and forgotten with the account. Asking GitHub again is in `profiles`, and what each version of client is sent in
//! `crate::server::subscribe`.

use protocol::model::{GithubAccount, GithubToken, GithubUsername, Observation, Observed};
use protocol::worker::{GitIdentity, GithubCredential};
use rusqlite::Connection;

use super::*;
use crate::hub::Hub;
use crate::hub::tests::{hub_calling_github, hub_calling_github_again};
use crate::store::BEFORE_GITHUB_USERS_OBSERVED;
use crate::store::columns::OneValue;
use crate::store::older::store_from_before;
use github::{Answer, StandIn};

/// A stand-in for GitHub that says `a-token` is ada's, who goes by Ada Lovelace and shows her email.
async fn github_of_ada() -> StandIn {
    let ada = Answer::user_with_profile(7, "ada", Some("Ada Lovelace"), Some("ada@example.com"));
    StandIn::start(vec![("a-token", ada)]).await
}

/// Connects ada's token, as a client asks, which makes her account the default, and returns its record.
async fn connected(hub: &Hub) -> GithubAccount {
    let asked = GithubAccountConnect { token: GithubToken::new("a-token"), id: None };
    hub.github_account_connect(asked).await.unwrap()
}

/// What `hub` lists of what GitHub calls `account`'s user, and the account's record.
async fn listed(hub: &Hub, account: &str) -> (Option<Observed<GithubUsername>>, Option<GithubAccount>) {
    let records = hub.records_list().await.unwrap();
    let observed = records.observations.into_iter().find_map(|observation| {
        let Observation::GithubUsername { account: of, username, .. } = observation else { return None };
        (of.as_str() == account).then_some(username)
    });
    (observed, records.github_accounts.into_iter().find(|record| record.id.as_str() == account))
}

/// A hub that starts again has what GitHub called the user as of the hub's read, fills the account's record with it,
/// and has workers commit as her profile said, before it asks GitHub again.
#[tokio::test]
async fn a_username_and_a_profile_are_kept_across_a_restart_as_of_their_read() {
    let github = github_of_ada().await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let account = connected(&hub).await;
    hub.observations_written().await;
    drop(hub);

    let again = hub_calling_github_again(dir.path(), github.api());

    let (observed, record) = listed(&again, account.id.as_str()).await;
    assert!(matches!(observed, Some(Observed::AsOf { value, .. }) if value.as_str() == "ada"));
    assert_eq!(record.unwrap().user.name.as_str(), "ada", "the record filled with it");
    let GithubCredential::Account(credential) = again.git_credential().borrow().for_workers() else { panic!() };
    let identity = GitIdentity { name: "Ada Lovelace".to_string(), email: "ada@example.com".to_string() };
    assert_eq!((credential.username.as_str(), credential.identity), ("ada", Some(identity)));
    assert_eq!(github.asked().len(), 1, "only as it was connected");
}

/// A database from before the username left it opens with the column and the profiles' table gone, and the record as
/// it was but for its user's name, which is its number until the hub hears what GitHub calls it, as nothing is carried
/// out of the database.
#[tokio::test]
async fn an_account_from_before_opens_with_its_username_not_known() {
    let dir = tempfile::tempdir().unwrap();
    let rows = "INSERT INTO github_accounts (id, user_id, username, app_id, installation_id, installed_on_id, installed_on)
                    VALUES ('g', 7, 'ada', 9, 70, 2, 'adalabs');
                INSERT INTO github_profiles (account_id, name, email, read_at_ms) VALUES ('g', 'Ada', 'a@example.com', 5);";
    drop(store_from_before(dir.path(), BEFORE_GITHUB_USERS_OBSERVED, rows));
    let github = github_of_ada().await;

    let hub = hub_calling_github_again(dir.path(), github.api());

    let db = Connection::open(dir.path().join("hub.db")).unwrap();
    let named = |sql: &str| -> Vec<String> {
        db.prepare(sql).unwrap().query_map([], OneValue::read).unwrap().map(Result::unwrap).collect()
    };
    let columns = named("SELECT name FROM pragma_table_info('github_accounts')");
    assert!(!columns.iter().any(|column| column == "username"), "{columns:?}");
    assert_eq!(named("SELECT name FROM sqlite_schema WHERE name = 'github_profiles'"), [] as [String; 0]);
    let (observed, record) = listed(&hub, "g").await;
    let record = record.expect("the record kept");
    assert_eq!(observed, None, "not known");
    assert_eq!((record.user.id.get(), record.user.name.as_str()), (7, "#7"));
    assert_eq!(record.installation.map(|installed| installed.on.name.to_string()).as_deref(), Some("adalabs"));
}

/// What GitHub calls the user goes with the account, as it's disconnected, and a hub that starts again keeps none of
/// it.
#[tokio::test]
async fn a_username_is_forgotten_with_its_account() {
    let github = github_of_ada().await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let account = connected(&hub).await;
    let while_connected = listed(&hub, account.id.as_str()).await.0;

    hub.github_account_disconnect(account.id.clone()).await.unwrap();
    hub.observations_written().await;
    let after = listed(&hub, account.id.as_str()).await;
    drop(hub);
    let again = hub_calling_github_again(dir.path(), github.api());

    assert!(while_connected.is_some());
    assert_eq!(after, (None, None));
    assert_eq!(listed(&again, account.id.as_str()).await, (None, None));
    assert!(!again.observations.keeps::<crate::hub::GithubProfile>(&account.id), "nor its profile");
}
