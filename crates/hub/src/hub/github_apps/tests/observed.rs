//! What GitHub says of an app of your own, its bot, and where each installation is, which the hub keeps as
//! observations, and fills their records with for a client of a version from before: as GitHub is asked again, across
//! a restart, from a database from before, with the app's key gone, and forgotten with the app.

use protocol::model::{GithubApp, GithubAppSaid, GithubUser, GithubUserId, Observation};
use rusqlite::Connection;

use super::making::{APP, MINUTE_MS, made, on_adalabs, token_ending};
use crate::github_logins::PROFILE_READ_EVERY;
use crate::hub::tests::{hub_calling_github, hub_calling_github_again};
use crate::hub::{GithubAppBot, GithubAppSaid as Said, Hub, InstalledOn};
use crate::store::BEFORE_APPS_OBSERVED;
use crate::store::columns::OneValue;
use crate::store::now_ms;
use crate::store::older::store_from_before;
use github::{Answer, Script, StandIn};

/// A stand-in as `making` has it, which answers `GET /app`, asked as the app, as `said` says, in turn.
async fn github_saying(said: Vec<Answer>) -> StandIn {
    StandIn::with(Script {
        conversions: vec![("a-code", Answer::app_made(9, "ada-roundhouse", (2, "adalabs")))],
        named: vec![("ada-roundhouse[bot]", Answer::user(41, "ada-roundhouse[bot]", None))],
        apps: said,
        installations: vec![on_adalabs()],
        minted: vec![(70, vec![token_ending("ghs", 60 * MINUTE_MS)])],
        ..Script::default()
    })
    .await
}

fn adalabs() -> GithubUser {
    GithubUser { id: GithubUserId::new(2), name: "adalabs".into() }
}

/// The app's record as the hub sends it, filled with what it holds.
async fn record(hub: &Hub) -> GithubApp {
    hub.app_record(APP).await.unwrap()
}

/// The only installation's account, as the hub sends it.
async fn installation(hub: &Hub) -> protocol::model::GithubAccount {
    let accounts = hub.records_list().await.unwrap().github_accounts;
    let [account] = accounts.as_slice() else { panic!("{accounts:?}") };
    account.clone()
}

/// Has the hub know nothing GitHub says of the app, nor where its installation is, as on the first start after it
/// stopped keeping them in its database.
async fn knowing_nothing(hub: &Hub) {
    let account = installation(hub).await.id;
    hub.observations.forget_for_a_test::<Said>(&APP);
    hub.observations.forget_for_a_test::<GithubAppBot>(&APP);
    hub.observations.forget_for_a_test::<InstalledOn>(&account);
}

/// The first look at an app the hub knows nothing of asks GitHub as the app, then for its bot by the slug GitHub said,
/// and keeps where its installation is, so its record is named, and so is its installation's.
#[tokio::test]
async fn a_look_at_an_app_known_nothing_of_asks_github_what_it_is_its_bot_and_where_its_installed() {
    let github = github_saying(vec![Answer::app_said(9, "ada-roundhouse", "Ada's Roundhouse", (2, "adalabs"))]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    made(&hub).await;
    knowing_nothing(&hub).await;
    let (before, on_before) = (record(&hub).await, installation(&hub).await);

    hub.check_github_logins(now_ms()).await;

    assert_eq!((before.slug.as_str(), before.name.as_str(), before.bot), ("#9", "#9", None));
    let on_before = on_before.installation.unwrap().on;
    assert_eq!(on_before, GithubUser { id: GithubUserId::new(2), name: "#2".into() }, "named by its number");
    let after = record(&hub).await;
    assert_eq!(
        (after.slug.as_str(), after.name.as_str(), &after.owner),
        ("ada-roundhouse", "Ada's Roundhouse", &adalabs())
    );
    assert_eq!(after.bot.map(|bot| bot.id.get()), Some(41));
    assert_eq!(installation(&hub).await.installation.unwrap().on, adalabs());
    assert_eq!(github.asked().iter().filter(|asked| asked.starts_with("get /app ")).count(), 1);
}

/// What GitHub says of the app is asked again a day after it was last, so an app renamed, or moved to another owner,
/// on GitHub shows as it is, where it never did while it was kept in the database.
#[tokio::test]
async fn an_app_renamed_on_github_shows_as_it_is_once_its_asked_again_a_day_on() {
    let github =
        github_saying(vec![Answer::app_said(9, "ada-roundhouse", "Ada's renamed", (3, "adalabs-moved"))]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    made(&hub).await;
    hub.check_github_logins(now_ms()).await;
    let within_the_day = record(&hub).await;
    let day_ms = i64::try_from(PROFILE_READ_EVERY.as_millis()).unwrap();
    let said = GithubAppSaid { slug: "ada-roundhouse".into(), name: within_the_day.name.clone(), owner: adalabs() };
    hub.observations.keep_as_read::<Said>(APP, said, now_ms() - day_ms);

    hub.check_github_logins(now_ms()).await;

    assert_eq!(within_the_day.name, "ada-roundhouse, made up", "as GitHub made it, not asked again within the day");
    let renamed = record(&hub).await;
    assert_eq!((renamed.name.as_str(), renamed.owner.name.as_str()), ("Ada's renamed", "adalabs-moved"));
}

/// A hub that starts again has what GitHub said of the app, its bot and where its installation is, as of when it was
/// said, and fills their records with them, before it asks GitHub again.
#[tokio::test]
async fn what_github_says_of_an_app_is_kept_across_a_restart() {
    let github = github_saying(Vec::new()).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let app = made(&hub).await;
    hub.observations_written().await;
    drop(hub);

    let again = hub_calling_github_again(dir.path(), github.api());

    assert_eq!(record(&again).await, GithubApp { rev: record(&again).await.rev, ..app });
    assert_eq!(installation(&again).await.installation.unwrap().on, adalabs());
}

/// A database from before what GitHub says of an app left it opens with the columns gone, the app's and its
/// installation's records as they were but for those, named by their numbers, and nothing of them known.
#[tokio::test]
async fn an_app_from_before_opens_with_what_github_says_of_it_not_known() {
    let dir = tempfile::tempdir().unwrap();
    let rows = "INSERT INTO github_apps (id, slug, name, owner_id, owner, bot_id, bot, key_file, made_from)
                    VALUES (9, 'ada-roundhouse', 'Ada', 2, 'adalabs', 41, 'ada-roundhouse[bot]', 'a-file', 'a-code');
                INSERT INTO github_accounts (id, user_id, app_id, installation_id, installed_on_id, installed_on)
                    VALUES ('g', 41, 9, 70, 2, 'adalabs');";
    drop(store_from_before(dir.path(), BEFORE_APPS_OBSERVED, rows));
    let github = github_saying(Vec::new()).await;

    let hub = hub_calling_github_again(dir.path(), github.api());

    let db = Connection::open(dir.path().join("hub.db")).unwrap();
    let columns = |table: &str| -> Vec<String> {
        let sql = format!("SELECT name FROM pragma_table_info('{table}')");
        db.prepare(&sql).unwrap().query_map([], OneValue::read).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(columns("github_apps"), ["id", "key_file", "made_from", "rev"]);
    assert!(!columns("github_accounts").iter().any(|column| column == "installed_on"));
    let app = record(&hub).await;
    assert_eq!((app.slug.as_str(), app.name.as_str(), app.owner.name.as_str(), app.bot), ("#9", "#9", "#0", None));
    assert_eq!(
        installation(&hub).await.installation.unwrap().on,
        GithubUser { id: GithubUserId::new(2), name: "#2".into() }
    );
    assert!(!hub.observations.keeps::<Said>(&APP), "not known");
}

/// An app whose key is gone can't be asked anything, so it's drawn by its number and can only be removed, which
/// works with nothing of it known.
#[tokio::test]
async fn an_app_known_nothing_of_whose_key_is_gone_is_named_by_its_number_and_can_be_removed() {
    let github = github_saying(Vec::new()).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    made(&hub).await;
    knowing_nothing(&hub).await;
    std::fs::remove_dir_all(dir.path().join("github-logins")).unwrap();

    let looked = hub.github_app_installations_connect(APP).await;
    let named = record(&hub).await;
    let removed = hub.github_app_remove(APP).await.unwrap();

    assert!(looked.is_err(), "{looked:?}");
    assert_eq!((named.slug.as_str(), named.name.as_str()), ("#9", "#9"));
    assert_eq!((removed.id, removed.name.as_str()), (APP, "#9"));
    let records = hub.records_list().await.unwrap();
    assert!(records.github_apps.is_empty());
    assert!(!records.observations.iter().any(|observation| matches!(observation, Observation::GithubAppSaid { .. })));
}

/// What GitHub says of an app goes with it: a hub that starts on a database without the app keeps none of it.
#[tokio::test]
async fn what_github_says_of_an_app_is_forgotten_with_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), crate::hub::tests::NO_GITHUB.to_string());
    let gone = protocol::model::GithubAppId::new(99);
    let said = GithubAppSaid { slug: "gone".into(), name: "Gone".into(), owner: adalabs() };
    hub.observation_read::<Said>(gone, said, 1_000).await.unwrap();
    hub.observations_written().await;
    drop(hub);

    let again = hub_calling_github_again(dir.path(), crate::hub::tests::NO_GITHUB.to_string());

    assert!(!again.observations.keeps::<Said>(&gone));
}
