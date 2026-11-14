//! Connecting and disconnecting as a client asks, and which login a launch gets, against a hub that never starts a
//! worker, and a stand-in for the machine's worker. What's kept, and in what order, and what's refused, is tested in
//! `crate::logins`. The daemon's `e2e/logins.rs` has launches under the hub's login through a real worker.

use protocol::model::{AgentDescription, AgentKind, LoginToken};
use tokio::sync::broadcast::error::TryRecvError;

use super::*;
use crate::hub::tests::{A_TOKEN_LASTS_MS, described, hub_describing, hub_in};

fn asking(name: &str, token: &str) -> AccountConnect {
    AccountConnect { agent: AgentKind::Claude, name: name.to_string(), token: LoginToken::new(token), account: None }
}

#[tokio::test]
async fn a_login_is_kept_under_an_id_the_hub_chose_and_ends_as_its_kind_says() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());

    let account = hub.account_connect(asking("Work", "a-made-up-token")).await.unwrap();

    assert!(account.id.as_str().starts_with("claude-login-"), "{}", account.id);
    let held = account.held_login.unwrap();
    assert_eq!(held.ends_at_ms, Some(held.connected_at_ms + A_TOKEN_LASTS_MS));
}

#[tokio::test]
async fn a_kind_the_hub_can_hold_no_login_for_and_one_it_doesnt_know_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let other = tempfile::tempdir().unwrap();
    let loginless = hub_describing(other.path(), vec![AgentDescription { login: None, ..described() }]);

    let newer_kind =
        hub.account_connect(AccountConnect { agent: AgentKind::Unknown, ..asking("Work", "a-token") }).await;
    let no_login = loginless.account_connect(asking("Work", "a-made-up-token")).await;

    assert_eq!(newer_kind.unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(no_login.unwrap_err().code, ErrorCode::InvalidRequest);
    assert!(!other.path().join("logins").exists(), "nothing was kept");
}

#[tokio::test]
async fn every_client_is_told_of_what_changed_an_accounts_record_and_only_that() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut feed = hub.subscribe_feed();

    let connected = hub.account_connect(asking("Work", "a-made-up-token")).await.unwrap();
    let told = feed.try_recv();
    let defaults = feed.try_recv();
    assert!(
        matches!(defaults, Ok(HubNotice::HubSettings(settings)) if settings.account_defaults.as_ref().unwrap()[0].account == connected.id)
    );
    hub.account_connect(asking("Work", "a-made-up-token")).await.unwrap();
    let told_again = feed.try_recv();
    let disconnected = hub.account_disconnect(connected.id.clone()).await.unwrap();
    let told_last = feed.try_recv();

    assert!(matches!(told, Ok(HubNotice::Account(record)) if record == connected));
    assert!(matches!(told_again, Err(TryRecvError::Empty)), "{told_again:?}");
    assert!(matches!(told_last, Ok(HubNotice::Account(record)) if record == disconnected));
}

#[tokio::test]
async fn a_launch_gets_the_chosen_account_without_consulting_any_machine_login() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    assert!(hub.login_for(AgentKind::Claude).await.unwrap().is_none());
    let first = hub.account_connect(asking("Work", "work-token")).await.unwrap();
    let other = hub.account_connect(asking("Personal", "personal-token")).await.unwrap();
    assert_eq!(hub.login_for(AgentKind::Claude).await.unwrap().unwrap().account, first.id);
    hub.account_set_default(protocol::client::accounts::AccountSetDefault {
        agent: AgentKind::Claude,
        account: other.id.clone(),
    })
    .await
    .unwrap();
    assert_eq!(hub.login_for(AgentKind::Claude).await.unwrap().unwrap().account, other.id);
    hub.account_disconnect(other.id).await.unwrap();
    assert_eq!(hub.login_for(AgentKind::Claude).await.unwrap_err().code, ErrorCode::Conflict);
}
