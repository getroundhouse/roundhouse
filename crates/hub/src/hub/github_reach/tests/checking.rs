//! When the hub checks each repository, with which login, and what it keeps of the answers.

use std::sync::Arc;

use protocol::client::github_accounts::GithubAccountConnect;
use protocol::client::github_apps::GithubManifestCode;
use protocol::model::{GithubToken, InstalledApp, Reach, UnusableLogin};
use protocol::worker::PushRoute;
use tokio::sync::watch;

use super::super::CHECKED_AGAIN_EVERY;
use super::support::{CLI, account, checking, reads, reads_pushing, told, told_within};
use crate::github::{Answer, Gate, Script, StandIn};
use crate::github_logins::DefaultCredential;
use crate::hub::Hub;
use crate::hub::tests::hub_calling_github;

fn refused_as(username: &str) -> Reach {
    let reason = "Permission to cli/cli.git denied.".into();
    Reach::Refused { repository: CLI, username: username.into(), reason, installation: None }
}

/// GitHub, refusing every push to `cli/cli`.
async fn refusing() -> StandIn {
    let refused = Answer::git_says(403, "Permission to cli/cli.git denied.");
    StandIn::with(Script { pushes: vec![("cli/cli", vec![refused])], ..Script::default() }).await
}

/// Has a worker read that the checkout of the hub's repository, `r`, fetches from `cli/cli` on GitHub.
async fn says_cli(hub: &Arc<Hub>) {
    reads(hub, "https://github.com/cli/cli.git").await;
}

/// What the last run recorded, as the hub starts.
fn left_by_the_last_run(hub: &Hub, reach: &Reach, answered_at_ms: i64) {
    hub.store.record_github_reach(&"r".into(), reach, Some(answered_at_ms)).unwrap();
    hub.store.github_reach_left_by_the_last_run().unwrap();
}

/// The repository's address is still to be read, as each is once a hub from before addresses is upgraded, so the last
/// run's record names which repository on GitHub to check, and no worker has to read it first. Once it's checked, it's
/// this run's, with when GitHub answered.
#[tokio::test]
async fn as_the_hub_starts_each_repository_the_last_run_recorded_is_checked_again_with_the_default() {
    let github = refusing().await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    left_by_the_last_run(&hub, &Reach::Pushes { repository: CLI, username: "grace".into() }, 5);
    let mut feed = hub.subscribe_feed();
    let (_credential, following) = watch::channel(account("ada", "a-token"));
    let started_ms = crate::store::now_ms();

    let _checking = checking(&hub, following);
    let reach = told(&mut feed).await;

    assert_eq!((reach.repo.as_str(), &reach.reach), ("r", &refused_as("ada")));
    assert!(!reach.stale && reach.answered_at_ms.is_some_and(|at| at >= started_ms), "{reach:?}");
    assert_eq!(hub.records_list().await.unwrap().github_reach, [reach]);
    assert!(github.asked()[0].starts_with("get /cli/cli.git/info/refs?service=git-receive-pack "));
}

/// GitHub doesn't answer, so the last run's answer stays as it was, with when GitHub gave it, still stale. One of
/// another login is no answer for this one, so not knowing replaces it.
#[tokio::test]
async fn a_check_github_doesnt_answer_keeps_what_it_last_answered_for_the_login() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), crate::hub::tests::NO_GITHUB.to_string());
    left_by_the_last_run(&hub, &refused_as("ada"), 5);
    let mut feed = hub.subscribe_feed();
    let (credential, following) = watch::channel(account("ada", "a-token"));
    let _checking = checking(&hub, following);

    let kept = told(&mut feed).await;
    credential.send_replace(account("grace", "g-token"));
    let unknown = told(&mut feed).await;

    assert_eq!((kept.reach, kept.answered_at_ms, kept.stale), (refused_as("ada"), Some(5), true));
    assert!(matches!(unknown.reach, Reach::CouldntTell { .. }), "{unknown:?}");
    assert_eq!((unknown.answered_at_ms, unknown.stale), (None, false));
}

/// A repository is checked once a worker reads its address, and every client is told, whatever the login.
#[tokio::test]
async fn a_repository_is_checked_once_a_worker_reads_its_address_and_every_client_told() {
    let github = refusing().await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let mut feed = hub.subscribe_feed();
    let (credential, following) = watch::channel(DefaultCredential::MachineOwn);
    let _checking = checking(&hub, following);

    says_cli(&hub).await;
    let machines_own = told(&mut feed).await;
    credential.send_replace(account("ada", "a-token"));
    let refused = told(&mut feed).await;

    assert_eq!(machines_own.reach, Reach::MachineOwn { repository: CLI });
    assert_eq!(refused.reach, refused_as("ada"));
    assert_eq!(github.asked().len(), 1, "only the account's was asked of GitHub");
}

/// Its address, once read, names which repository on GitHub it is, over what the last run checked, so one elsewhere
/// isn't on GitHub, and isn't asked of it. Added again, its address is read again, and a new one is checked for.
#[tokio::test]
async fn a_repository_is_checked_for_what_its_address_names_as_each_is_read() {
    let github = refusing().await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    left_by_the_last_run(&hub, &refused_as("ada"), 5);
    reads(&hub, "https://example.com/ada/roundhouse.git").await;
    let mut feed = hub.subscribe_feed();
    let (_credential, following) = watch::channel(account("ada", "a-token"));
    let _checking = checking(&hub, following);

    let elsewhere = told(&mut feed).await;
    hub.store.add_repo(&"again".into(), "repo", "/repo", 0).unwrap();
    reads(&hub, "git@github.com:cli/cli.git").await;
    let on_github = told(&mut feed).await;

    assert_eq!(elsewhere.reach, Reach::NotOnGithub);
    assert_eq!(on_github.reach, refused_as("ada"));
    assert_eq!(github.asked().len(), 1, "only cli/cli was asked of GitHub");
}

/// A worker says its pushes go round the git credential, as over SSH, so agents push as the machine's own whoever
/// the default is, past a block too, and GitHub isn't asked. Added again and read as going through it, it's checked
/// with the default. The feed tells of each record only as it changes, so each step changes it.
#[tokio::test]
async fn a_repository_whose_pushes_go_round_the_credential_is_said_to_push_as_the_machines_own() {
    let github = refusing().await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let mut feed = hub.subscribe_feed();
    let why = UnusableLogin::Disconnected;
    let blocked = DefaultCredential::Blocked { reason: "ada was disconnected".into(), account: Some("g".into()), why };
    let (credential, following) = watch::channel(blocked);
    let _checking = checking(&hub, following);

    reads_pushing(&hub, "https://github.com/cli/cli.git", PushRoute::Around).await;
    let past_the_block = told(&mut feed).await;
    credential.send_replace(DefaultCredential::MachineOwn);
    let machines_own = told(&mut feed).await;
    credential.send_replace(account("ada", "a-token"));
    let as_an_account = told(&mut feed).await;
    hub.store.add_repo(&"again".into(), "repo", "/repo", 0).unwrap();
    reads(&hub, "https://github.com/cli/cli.git").await;
    let through = told(&mut feed).await;

    assert_eq!(past_the_block.reach, Reach::PushesAsMachineOwn { repository: CLI });
    assert_eq!(machines_own.reach, Reach::MachineOwn { repository: CLI });
    assert_eq!(as_an_account.reach, Reach::PushesAsMachineOwn { repository: CLI });
    assert_eq!(through.reach, refused_as("ada"));
    assert_eq!(github.asked().len(), 1, "only once it went through the credential");
}

/// As when another default is chosen, and as when the default's sign-in is renewed, which changes its token, and so
/// can change what it can push to.
#[tokio::test]
async fn a_new_default_or_a_renewed_login_has_every_repository_checked_again_with_it() {
    let github = refusing().await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let mut feed = hub.subscribe_feed();
    let (credential, following) = watch::channel(account("ada", "a-token"));
    let _checking = checking(&hub, following);
    says_cli(&hub).await;
    let ada = told(&mut feed).await;

    credential.send_replace(account("grace", "g-token"));
    let grace = told(&mut feed).await;
    credential.send_replace(account("grace", "g-renewed"));
    github.until_asked(3).await;

    assert_eq!((ada.reach, grace.reach), (refused_as("ada"), refused_as("grace")));
    // `grace:g-renewed`.
    assert!(github.asked()[2].contains("authorization: basic z3jhy2u6zy1yzw5ld2vk\r\n"), "{}", github.asked()[2]);
}

/// GitHub answers the first login's check only once the default has changed twice, so what it says is the old login's,
/// and never recorded. The next login's is.
#[tokio::test]
async fn an_answer_for_a_login_that_has_since_changed_is_let_go_of() {
    let gate = Gate::closed();
    let held = Answer::After(gate.clone(), Box::new(Answer::takes_pushes()));
    let github = StandIn::with(Script { pushes: vec![("cli/cli", vec![held])], ..Script::default() }).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let mut feed = hub.subscribe_feed();
    let (credential, following) = watch::channel(account("ada", "a-token"));
    let _checking = checking(&hub, following);
    says_cli(&hub).await;
    github.until_asked(1).await;

    credential.send_replace(DefaultCredential::MachineOwn);
    let machines_own = told(&mut feed).await;
    gate.open();
    credential.send_replace(account("grace", "g-token"));
    let grace = told(&mut feed).await;

    assert_eq!(machines_own.reach, Reach::MachineOwn { repository: CLI });
    assert_eq!(grace.reach, Reach::Pushes { repository: CLI, username: "grace".into() });
    assert_eq!(hub.records_list().await.unwrap().github_reach, [grace]);
}

/// What's recorded is changed behind the hub's back, so checking again changes it back, which the feed tells of. Time
/// moves only by tokio's paused clock.
#[tokio::test]
async fn every_half_hour_each_repository_is_checked_again() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), crate::hub::tests::NO_GITHUB.to_string());
    let mut feed = hub.subscribe_feed();
    let (_credential, following) = watch::channel(DefaultCredential::MachineOwn);
    let _checking = checking(&hub, following);
    says_cli(&hub).await;
    told(&mut feed).await;
    hub.store.record_github_reach(&"r".into(), &Reach::NotOnGithub, None).unwrap();

    tokio::time::pause();
    let started = tokio::time::Instant::now();
    let again = told_within(&mut feed, CHECKED_AGAIN_EVERY * 2).await;

    assert_eq!(again.reach, Reach::MachineOwn { repository: CLI });
    assert!(started.elapsed() >= CHECKED_AGAIN_EVERY / 2, "checked again after {:?}", started.elapsed());
}

/// The hub's own checks follow what its logins have workers sent, so connecting the first account, which makes it the
/// default, has it checked with that account's token, and named by its username, not the name it commits as.
#[tokio::test]
async fn the_hub_checks_with_the_default_its_logins_work_out() {
    let github = StandIn::with(Script {
        users: vec![("a-token", Answer::user_with_profile(7, "ada", Some("Ada Lovelace"), None))],
        pushes: vec![("cli/cli", vec![Answer::takes_pushes()])],
        ..Script::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let mut feed = hub.subscribe_feed();
    let _checking = tokio::spawn(Arc::clone(&hub).watch_github_reach());

    let connect = GithubAccountConnect { token: GithubToken::new("a-token"), id: None };
    hub.github_account_connect(connect).await.unwrap();
    says_cli(&hub).await;
    let reach = told(&mut feed).await;

    assert_eq!(reach.reach, Reach::Pushes { repository: CLI, username: "ada".into() });
}

/// An app of your own's installation on adalabs, which making the app makes the default, is refused `cli/cli`, which
/// isn't adalabs's, and the record names its bot, and where its app is installed, not the `x-access-token` git gives.
#[tokio::test]
async fn an_installation_the_default_refused_a_repository_is_named_as_its_bot() {
    let ends = chrono::DateTime::from_timestamp_millis(crate::store::now_ms() + 60 * 60 * 1000).unwrap();
    let denied = "Permission to cli/cli.git denied to ada-roundhouse[bot].";
    let github = StandIn::with(Script {
        conversions: vec![("a-code", Answer::app_made(9, "ada-roundhouse", (2, "adalabs")))],
        named: vec![("ada-roundhouse[bot]", Answer::user(41, "ada-roundhouse[bot]", None))],
        installations: vec![Answer::installations(&[(70, 2, "adalabs")])],
        minted: vec![(70, vec![Answer::installation_token("ghs_first", &ends.to_rfc3339())])],
        pushes: vec![("cli/cli", vec![Answer::git_says(403, denied)])],
        ..Script::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    hub.github_app_make(GithubManifestCode::new("a-code")).await.unwrap();
    let mut feed = hub.subscribe_feed();
    let _checking = tokio::spawn(Arc::clone(&hub).watch_github_reach());

    says_cli(&hub).await;
    let reach = told(&mut feed).await;

    let installation = Some(InstalledApp { slug: "ada-roundhouse".into(), on: "adalabs".into() });
    let username = "ada-roundhouse[bot]".into();
    assert_eq!(reach.reach, Reach::Refused { repository: CLI, username, reason: denied.into(), installation });
}
