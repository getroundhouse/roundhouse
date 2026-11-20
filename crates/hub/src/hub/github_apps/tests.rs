//! An app of your own, as a client asks and as the hub's check does, against a hub that never starts a worker and a
//! stand-in for GitHub, on tokio's clock as it runs, since a request to the stand-in races a paused clock. Which
//! installations are connected, forgotten and due, and what each of GitHub's answers keeps, is tested in
//! `crate::github_logins`, and what GitHub is asked, and what its answers come to, in the `github` crate.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::github_apps::GithubManifestCode;
use protocol::client::{ErrorCode, HubNotice};
use protocol::model::{GithubApp, GithubAppId, GithubToken, RecordKey};
use protocol::worker::{GitCredential, GitIdentity, GithubCredential};
use tokio::sync::broadcast;

use crate::hub::Hub;
use crate::hub::tests::hub_calling_github;
use crate::store::now_ms;
use github::{Answer, Gate, Script, StandIn};

const APP: GithubAppId = GithubAppId::new(9);
const MINUTE_MS: i64 = 60 * 1000;

/// A stand-in that makes the app 9 from `a-code`, under the organization adalabs, names its bot, lists it installed
/// on adalabs, as 70, in turn as `listed` says, and makes that installation `tokens` in turn.
async fn github(listed: Vec<Answer>, tokens: Vec<Answer>) -> StandIn {
    StandIn::with(Script {
        conversions: vec![("a-code", Answer::app_made(9, "ada-roundhouse", (2, "adalabs")))],
        named: vec![("ada-roundhouse[bot]", Answer::user(41, "ada-roundhouse[bot]", None))],
        installations: listed,
        minted: vec![(70, tokens)],
        ..Script::default()
    })
    .await
}

fn on_adalabs() -> Answer {
    Answer::installations(&[(70, 2, "adalabs")])
}

/// The token `token`, ending `in_ms` from now, as GitHub gives an installation's.
fn token_ending(token: &str, in_ms: i64) -> Answer {
    let ends = jiff::Timestamp::from_millisecond(now_ms() + in_ms).unwrap();
    Answer::installation_token(token, &ends.to_string())
}

async fn made(hub: &Hub) -> GithubApp {
    hub.github_app_make(GithubManifestCode::new("a-code")).await.unwrap()
}

/// Every notice in `feed` so far.
fn told(feed: &mut broadcast::Receiver<HubNotice>) -> Vec<HubNotice> {
    std::iter::from_fn(|| feed.try_recv().ok()).collect()
}

fn sent(hub: &Hub) -> GithubCredential {
    hub.git_credential().borrow().for_workers()
}

/// What workers are sent while the default is the app's installation on adalabs, whose token is `token`.
fn bots(token: &str) -> GithubCredential {
    let identity = GitIdentity {
        name: "ada-roundhouse[bot]".into(),
        email: "41+ada-roundhouse[bot]@users.noreply.github.com".into(),
    };
    GithubCredential::Account(GitCredential {
        username: "x-access-token".into(),
        token: GithubToken::new(token),
        identity: Some(identity),
    })
}

fn blocked(hub: &Hub) -> String {
    let GithubCredential::Blocked { reason } = sent(hub) else { panic!("not a block: {:?}", sent(hub)) };
    reason
}

#[tokio::test]
async fn making_an_app_keeps_it_connects_where_its_installed_and_workers_commit_and_push_as_its_bot() {
    let github = github(vec![on_adalabs()], vec![token_ending("ghs_first", 60 * MINUTE_MS)]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let mut feed = hub.subscribe_feed();

    let app = made(&hub).await;
    let asked = github.asked().len();
    let again = made(&hub).await;

    assert_eq!(app.bot.as_ref().map(|bot| (bot.id.get(), bot.name.as_str())), Some((41, "ada-roundhouse[bot]")));
    assert_eq!(again, app, "the same code again finds the app it made");
    assert_eq!((asked, github.asked().len()), (4, 4), "making it, its bot, its installations and a token, and once");
    let told = told(&mut feed);
    let [HubNotice::GithubApp(kept), HubNotice::GithubApp(with_bot), HubNotice::GithubAccount(account), ..] =
        told.as_slice()
    else {
        panic!("{told:?}")
    };
    assert_eq!((kept.bot.as_ref(), with_bot), (None, &app));
    assert_eq!(account.installation.as_ref().map(|installed| installed.id.get()), Some(70));
    assert!(told.iter().any(|notice| matches!(notice, HubNotice::HubSettings(_))), "it became the default: {told:?}");
    assert_eq!(sent(&hub), bots("ghs_first"));
}

#[tokio::test]
async fn a_code_github_doesnt_know_makes_no_app() {
    let github = github(vec![on_adalabs()], Vec::new()).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());

    let refused = hub.github_app_make(GithubManifestCode::new("another-code")).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::InvalidRequest);
    assert!(hub.store.list_records().unwrap().github_apps.is_empty());
}

/// A GitHub that can't list where it's installed yet, as when the person hasn't installed it, or GitHub is down.
#[tokio::test]
async fn an_app_whose_installations_cant_be_listed_is_made_and_connected_once_the_person_asks() {
    let listed = vec![Answer::status(503, Vec::new()), on_adalabs()];
    let github = github(listed, vec![token_ending("ghs_first", 60 * MINUTE_MS)]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());

    made(&hub).await;
    let before = sent(&hub);
    let app = hub.github_app_installations_connect(APP).await.unwrap();

    assert!(matches!(before, GithubCredential::Blocked { .. }), "{before:?}");
    assert_eq!(app.id, APP);
    assert_eq!(sent(&hub), bots("ghs_first"));
}

/// The first token has 10 minutes left, under the 20 the hub makes the next with, so the first check makes it.
#[tokio::test]
async fn a_check_makes_an_installations_next_token_before_the_last_ends_and_workers_are_sent_it() {
    let tokens = vec![token_ending("ghs_first", 10 * MINUTE_MS), token_ending("ghs_next", 60 * MINUTE_MS)];
    let github = github(vec![on_adalabs()], tokens).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    made(&hub).await;
    let first = sent(&hub);

    hub.check_github_logins(now_ms()).await;

    assert_eq!((first, sent(&hub)), (bots("ghs_first"), bots("ghs_next")));
    let minted = github.asked().iter().filter(|asked| asked.starts_with("post /app/installations/70/")).count();
    assert_eq!(minted, 2, "the check made one, and its look connected nothing it held already");
}

/// GitHub lists only where the app is installed now, so the next check, which looks, forgets the installation.
#[tokio::test]
async fn an_app_uninstalled_where_it_was_is_forgotten_by_the_next_check_which_tells_every_client_and_blocks() {
    let github =
        github(vec![on_adalabs(), Answer::installations(&[])], vec![token_ending("ghs", 60 * MINUTE_MS)]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    made(&hub).await;
    let mut feed = hub.subscribe_feed();

    hub.check_github_logins(now_ms()).await;

    let told = told(&mut feed);
    let [HubNotice::GithubAccount(account)] = told.as_slice() else { panic!("{told:?}") };
    assert_eq!(account.held_login, None);
    assert!(blocked(&hub).contains("the app uninstalled from"));
}

#[tokio::test]
async fn removing_an_app_forgets_it_and_its_installations_logins_and_tells_every_client() {
    let github = github(vec![on_adalabs()], vec![token_ending("ghs", 60 * MINUTE_MS)]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    made(&hub).await;
    let mut feed = hub.subscribe_feed();

    let removed = hub.github_app_remove(APP).await.unwrap();
    let again = hub.github_app_remove(APP).await.unwrap_err();

    assert_eq!((removed.id, again.code), (APP, ErrorCode::NotFound));
    let told = told(&mut feed);
    let [HubNotice::GithubAccount(account), HubNotice::Removed { record, .. }] = told.as_slice() else {
        panic!("{told:?}")
    };
    assert_eq!((account.held_login, record), (None, &RecordKey::GithubApp { id: APP }));
    assert!(blocked(&hub).contains("was disconnected"));
}

/// GitHub holding back where the app is installed, as the first check looks, until after the stop. A stop that waited
/// would wait for the call's 15 s, so each wait here fails after 5 s rather than wait on.
#[tokio::test]
async fn a_stop_while_the_check_looks_for_where_an_app_is_installed_waits_for_no_answer() {
    let gate = Gate::closed();
    let held_back = Answer::After(gate.clone(), Box::new(on_adalabs()));
    let github = github(vec![on_adalabs(), held_back], vec![token_ending("ghs", 60 * MINUTE_MS)]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    made(&hub).await;
    let watching = tokio::spawn(Arc::clone(&hub).watch_github_logins());
    let looking = tokio::time::timeout(Duration::from_secs(5), github.until_asked(5)).await;

    let stopping = tokio::time::timeout(Duration::from_secs(5), hub.stop_checking_github_logins(watching)).await;

    assert!(looking.is_ok(), "the check asked GitHub {} times, not where the app is installed", github.asked().len());
    assert!(stopping.is_ok(), "the stop waited for GitHub");
    gate.open();
}
