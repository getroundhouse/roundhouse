//! Connecting and disconnecting as a client asks, and which login a launch gets, against a hub that never starts a
//! worker, and a stand-in for the machine's worker. What's kept, and in what order, and what's refused, is tested in
//! `crate::logins`. The daemon's `e2e/logins.rs` has launches under the hub's login through a real worker.

use std::sync::atomic::{AtomicUsize, Ordering};

use protocol::model::{AgentAccount, AgentDescription, AgentKind, LoginToken};
use protocol::worker::{WorkerRequest, WorkerResponse};
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
    hub.account_connect(asking("Work", "a-made-up-token")).await.unwrap();
    let told_again = feed.try_recv();
    let disconnected = hub.account_disconnect(connected.id.clone()).await.unwrap();
    let told_last = feed.try_recv();

    assert!(matches!(told, Ok(HubNotice::Account(record)) if record == connected));
    assert!(matches!(told_again, Err(TryRecvError::Empty)), "{told_again:?}");
    assert!(matches!(told_last, Ok(HubNotice::Account(record)) if record == disconnected));
}

/// A stand-in for the worker of a machine that is logged in or isn't, which counts how often it's asked, and fails the
/// test if it's asked anything but whose its own login is.
fn machine(logged_in: bool, asked: Arc<AtomicUsize>) -> Arc<WorkerLink> {
    WorkerLink::answering(move |request| {
        let WorkerRequest::AccountFor(AccountFor { login: None, .. }) = request else { panic!("asked {request:?}") };
        asked.fetch_add(1, Ordering::SeqCst);
        Some(Ok(WorkerResponse::Account(AgentAccount { logged_in, ..AgentAccount::default() })))
    })
}

#[tokio::test]
async fn a_hub_that_holds_no_login_hands_none_and_asks_the_machine_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let asked = Arc::new(AtomicUsize::new(0));

    let given = hub.login_for(&machine(false, asked.clone()), AgentKind::Claude).await.unwrap();

    assert!(given.is_none());
    assert_eq!(asked.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_machine_with_its_own_login_is_handed_none_and_one_without_is_handed_the_hubs() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let connected = hub.account_connect(asking("Work", "a-made-up-token")).await.unwrap();
    let asked = Arc::new(AtomicUsize::new(0));

    let logged_in = hub.login_for(&machine(true, asked.clone()), AgentKind::Claude).await.unwrap();
    let logged_out = hub.login_for(&machine(false, asked.clone()), AgentKind::Claude).await.unwrap().unwrap();

    assert!(logged_in.is_none());
    assert_eq!((logged_out.account, logged_out.login.token.as_str()), (connected.id, "a-made-up-token"));
    assert_eq!(asked.load(Ordering::SeqCst), 2, "each machine was asked");
}

#[tokio::test]
async fn a_machine_that_cant_say_whether_it_has_a_login_is_handed_the_hubs() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let connected = hub.account_connect(asking("Work", "a-made-up-token")).await.unwrap();
    let cant_say = WorkerLink::answering(|_| Some(Err(ApiError::new(ErrorCode::Worker, "the CLI timed out"))));

    let given = hub.login_for(&cant_say, AgentKind::Claude).await.unwrap().unwrap();

    assert_eq!(given.account, connected.id);
}

/// As when something else removed the hub's file: only a launch that would be handed the login fails over it.
#[tokio::test]
async fn a_machine_with_its_own_login_is_handed_none_even_when_the_hubs_token_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.account_connect(asking("Work", "a-made-up-token")).await.unwrap();
    for file in std::fs::read_dir(dir.path().join("logins")).unwrap() {
        std::fs::remove_file(file.unwrap().path()).unwrap();
    }
    let asked = Arc::new(AtomicUsize::new(0));

    let logged_in = hub.login_for(&machine(true, asked.clone()), AgentKind::Claude).await;
    let logged_out = hub.login_for(&machine(false, asked), AgentKind::Claude).await;

    assert!(logged_in.unwrap().is_none());
    assert_eq!(logged_out.unwrap_err().code, ErrorCode::Internal);
}
