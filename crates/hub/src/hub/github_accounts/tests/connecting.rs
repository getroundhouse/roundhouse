//! Connecting, disconnecting and choosing the default as a client asks, against a hub that never starts a worker, and a
//! stand-in for GitHub. What's kept, and in what order, and what's refused, is tested in `crate::github_logins`, and
//! what GitHub's answers come to in the `github` crate.

use std::sync::Arc;
use std::time::Duration;

use protocol::model::{GithubToken, RecordKey};
use protocol::worker::{GitIdentity, GithubCredential};
use tokio::sync::broadcast::error::TryRecvError;

use super::*;
use crate::hub::tests::hub_calling_github;
use github::{Answer, StandIn};

fn asking(token: &str) -> GithubAccountConnect {
    GithubAccountConnect { token: GithubToken::new(token), id: None }
}

#[tokio::test]
async fn every_client_is_told_of_what_changed_a_github_accounts_record_and_the_default_and_only_that() {
    let github = StandIn::start(vec![("a-token", Answer::user(7, "ada", None))]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let mut feed = hub.subscribe_feed();

    let connected = hub.github_account_connect(asking("a-token")).await.unwrap();
    let (told, made_default) =
        (feed.try_recv().map(std::sync::Arc::unwrap_or_clone), feed.try_recv().map(std::sync::Arc::unwrap_or_clone));
    let again = hub.github_account_connect(asking(" a-token\n")).await.unwrap();
    let told_again = feed.try_recv().map(std::sync::Arc::unwrap_or_clone);
    let machines_own = hub.github_account_set_default(GithubAccountSetDefault { account: None }).await.unwrap();
    let told_of_settings = feed.try_recv().map(std::sync::Arc::unwrap_or_clone);
    let disconnected = hub.github_account_disconnect(connected.id.clone()).await.unwrap();
    let told_last = feed.try_recv().map(std::sync::Arc::unwrap_or_clone);

    assert_eq!(connected.user.name.as_str(), "ada");
    assert!(matches!(told, Ok(HubNotice::GithubAccount(record)) if record == connected));
    let default = |notice: Result<HubNotice, TryRecvError>| match notice {
        Ok(HubNotice::HubSettings(settings)) => settings.github.map(|github| github.account),
        other => panic!("{other:?}"),
    };
    assert_eq!(default(made_default), Some(Some(connected.id.clone())));
    assert_eq!(again, connected);
    assert!(matches!(told_again, Err(TryRecvError::Empty)), "{told_again:?}");
    assert_eq!(github.asked().len(), 1, "the same token again didn't ask GitHub");
    assert!(matches!(told_of_settings, Ok(HubNotice::HubSettings(settings)) if settings == machines_own));
    assert_eq!(disconnected, connected, "the reply is the record as it last was");
    let removed = RecordKey::GithubAccount { id: connected.id.clone() };
    assert!(matches!(&told_last, Ok(HubNotice::Removed { record, .. }) if *record == removed), "{told_last:?}");
    assert!(
        matches!(feed.try_recv().map(std::sync::Arc::unwrap_or_clone), Err(TryRecvError::Empty)),
        "no default, so the settings didn't change"
    );
}

#[tokio::test]
async fn disconnecting_the_default_tells_every_client_its_gone_and_then_that_theres_no_default() {
    let github = StandIn::start(vec![("a-token", Answer::user(7, "ada", None))]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let connected = hub.github_account_connect(asking("a-token")).await.unwrap();
    let mut feed = hub.subscribe_feed();

    hub.github_account_disconnect(connected.id.clone()).await.unwrap();
    let (gone, settings) =
        (feed.try_recv().map(std::sync::Arc::unwrap_or_clone), feed.try_recv().map(std::sync::Arc::unwrap_or_clone));
    let again = hub.github_account_disconnect(connected.id.clone()).await;

    let Ok(HubNotice::Removed { record, rev }) = gone else { panic!("{gone:?}") };
    assert_eq!(record, RecordKey::GithubAccount { id: connected.id.clone() });
    assert!(rev.supersedes(connected.rev));
    let Ok(HubNotice::HubSettings(settings)) = settings else { panic!("{settings:?}") };
    assert_eq!(settings.github.map(|github| github.account), Some(None));
    assert_eq!(again.unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(hub.records_list().await.unwrap().github_accounts, []);
}

/// As `GET /user` says, when the hub asks whose the token is.
#[tokio::test]
async fn a_connected_default_commits_as_its_users_name_and_public_email_on_github() {
    let ada = Answer::user_with_profile(7, "ada", Some("Ada Lovelace"), Some("ada@example.com"));
    let github = StandIn::start(vec![("a-token", ada)]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());

    hub.github_account_connect(asking("a-token")).await.unwrap();

    let GithubCredential::Account(credential) = hub.git_credential().borrow().for_workers() else { panic!() };
    let identity = GitIdentity { name: "Ada Lovelace".to_string(), email: "ada@example.com".to_string() };
    assert_eq!((credential.username.as_str(), credential.identity), ("ada", Some(identity)));
    assert_eq!(github.asked().len(), 1, "who it is came with whose the token is");
}

#[tokio::test]
async fn a_token_github_refuses_keeps_nothing_and_tells_no_one() {
    let github = StandIn::start(Vec::new()).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let mut feed = hub.subscribe_feed();

    let refused = hub.github_account_connect(asking("a-revoked-token")).await;

    assert_eq!(refused.unwrap_err().code, ErrorCode::InvalidRequest);
    assert!(matches!(feed.try_recv().map(std::sync::Arc::unwrap_or_clone), Err(TryRecvError::Empty)));
    assert_eq!(hub.records_list().await.unwrap().github_accounts, []);
    assert!(!dir.path().join("github-logins").exists(), "nothing was kept");
}

#[tokio::test]
async fn a_github_that_doesnt_answer_holds_up_only_the_connect_waiting_on_it() {
    let answers = vec![("a-token", Answer::user(7, "ada", None)), ("a-token-github-sits-on", Answer::Never)];
    let github = StandIn::start(answers).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let ada = hub.github_account_connect(asking("a-token")).await.unwrap();
    let waiting = tokio::spawn({
        let hub = Arc::clone(&hub);
        async move { hub.github_account_connect(asking("a-token-github-sits-on")).await }
    });
    github.until_asked(2).await;

    // Bounded only so a disconnect that waits on the other connect fails here, rather than once GitHub's time is up.
    let disconnecting = hub.github_account_disconnect(ada.id.clone());
    let disconnected = tokio::time::timeout(Duration::from_secs(5), disconnecting).await;

    assert_eq!(disconnected.expect("the disconnect waited on the other connect").unwrap().id, ada.id);
    assert!(!waiting.is_finished(), "the other connect is still waiting on GitHub");
}

#[tokio::test]
async fn tidying_the_logins_as_the_hub_starts_tidies_github_accounts_too() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), crate::hub::tests::NO_GITHUB.to_string());
    let github_logins = dir.path().join("github-logins");
    std::fs::create_dir(&github_logins).unwrap();
    std::fs::write(github_logins.join(crate::login_files::LoginFile::new().as_str()), "a-stray-token").unwrap();

    hub.tidy_logins().await;

    assert_eq!(std::fs::read_dir(&github_logins).unwrap().count(), 0, "a file no account names is gone");
}
