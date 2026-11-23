//! Renewing as the hub's check does, against a hub that never starts a worker and a stand-in for GitHub, on tokio's
//! clock as it runs, since a request to the stand-in races a paused clock. Which sign-ins are due, what each of
//! GitHub's answers keeps, and what renewing one that has changed meanwhile comes to, is tested in
//! `crate::github_logins`, and what GitHub is asked, and what its answers come to, in the `github` crate.

use std::path::Path;
use std::sync::Arc;

use protocol::model::{GithubToken, SentCredential, UnusableLogin};
use protocol::worker::{GitCredential, GitIdentity, GithubCredential};
use tokio::sync::broadcast;

use super::*;
use crate::github_logins::tests::{ada_signed_in, login_ending};
use crate::hub::Hub;
use crate::hub::tests::hub_calling_github;
use crate::store::now_ms;
use github::{Answer, Script, StandIn};

const HOUR_MS: i64 = 60 * 60 * 1000;

/// A hub calling `github`, whose default is ada, signed in, with `a-token`, renewed with `a-token-refresh`, which has
/// half an hour left, so the first check renews it.
pub(super) fn hub_signed_in(dir: &Path, github: &StandIn) -> Arc<Hub> {
    let hub = hub_calling_github(dir, github.api());
    ada_signed_in(&hub.github_logins, &hub.store, login_ending("a-token", now_ms() + HOUR_MS / 2));
    hub
}

/// A stand-in for GitHub that answers each renewal with `a-token-refresh` with `answers`, the last again and again.
pub(super) async fn renewing(answers: Vec<Answer>) -> StandIn {
    StandIn::with(Script { renewals: vec![("a-token-refresh", answers)], ..Script::default() }).await
}

/// Every notice in `feed` so far.
fn told(feed: &mut broadcast::Receiver<std::sync::Arc<HubNotice>>) -> Vec<HubNotice> {
    std::iter::from_fn(|| feed.try_recv().ok().map(std::sync::Arc::unwrap_or_clone)).collect()
}

fn sent(hub: &Hub) -> GithubCredential {
    hub.git_credential().borrow().for_workers()
}

/// Ada's `token`, which commits as ada, at the address GitHub gives her, since she shows no name or email on GitHub.
fn as_ada(token: &str) -> GithubCredential {
    let identity = GitIdentity { name: "ada".to_string(), email: "7+ada@users.noreply.github.com".to_string() };
    GithubCredential::Account(GitCredential {
        username: "ada".into(),
        token: GithubToken::new(token),
        identity: Some(identity),
    })
}

#[tokio::test]
async fn a_check_renews_a_sign_in_thats_due_and_tells_every_client_of_its_record_and_every_worker_of_its_token() {
    let github = renewing(vec![Answer::granted("a-renewed-token", "a-renewed-refresh")]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_signed_in(dir.path(), &github);
    let mut feed = hub.subscribe_feed();
    let checked_at_ms = now_ms();

    hub.check_github_logins(checked_at_ms).await;

    let told = told(&mut feed);
    let [HubNotice::GithubAccount(account)] = told.as_slice() else { panic!("{told:?}") };
    let ends_at_ms = account.held_login.unwrap().ends_at_ms.unwrap();
    assert!(ends_at_ms >= checked_at_ms + 8 * HOUR_MS, "it ends 8 hours from when GitHub gave it, not {ends_at_ms}");
    assert_eq!(hub.store.github_account(&account.id).unwrap().as_ref(), Some(account));
    assert_eq!(sent(&hub), as_ada("a-renewed-token"));
    let asked = github.asked();
    assert_eq!(asked.len(), 1, "GitHub was asked once, and the check waited for its answer");
    assert!(asked[0].contains(r#""refresh_token":"a-token-refresh""#), "{}", asked[0]);
}

/// As GitHub answers what renews a token that has ended, been revoked or been used already.
#[tokio::test]
async fn a_check_has_a_sign_in_github_wont_renew_need_signing_in_again_tells_every_client_and_blocks_its_default() {
    let github = renewing(vec![Answer::device_flow_error("bad_refresh_token")]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_signed_in(dir.path(), &github);
    let mut feed = hub.subscribe_feed();

    hub.check_github_logins(now_ms()).await;

    let told = told(&mut feed);
    let [HubNotice::GithubAccount(account)] = told.as_slice() else { panic!("{told:?}") };
    assert!(account.held_login.unwrap().needs_sign_in);
    let GithubCredential::Blocked { reason } = sent(&hub) else { panic!("{:?}", sent(&hub)) };
    assert!(reason.contains("Sign in with GitHub again"), "{reason}");
}

/// GitHub failing, as it may not by the next try, with half an hour left, and then at a check two hours on.
#[tokio::test]
async fn a_renewal_that_may_pass_leaves_the_sign_in_as_it_is_and_past_its_end_its_default_blocks_saying_its_renewing() {
    let github = renewing(vec![Answer::status(503, Vec::new())]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_signed_in(dir.path(), &github);
    let before = hub.store.list_records().unwrap().github_accounts;
    let mut feed = hub.subscribe_feed();

    hub.check_github_logins(now_ms()).await;
    let in_time = sent(&hub);
    hub.check_github_logins(now_ms() + 2 * HOUR_MS).await;

    assert_eq!(github.asked().len(), 2, "each check asked GitHub");
    assert_eq!(told(&mut feed), [], "no account's record changed");
    assert_eq!(hub.store.list_records().unwrap().github_accounts, before);
    assert_eq!(in_time, as_ada("a-token"));
    let GithubCredential::Blocked { reason } = sent(&hub) else { panic!("{:?}", sent(&hub)) };
    assert!(reason.contains("has ended, and Roundhouse is renewing it"), "{reason}");
}

/// The settings that say so, once the hub follows them as it does as it serves, as a renewal that fails leaves the
/// default blocked past its renewed token's end, and the same settings as a renewal changes only the token.
#[tokio::test]
async fn every_client_is_told_of_the_settings_as_what_workers_are_sent_changes_and_not_as_a_renewal_does() {
    let renewals = vec![
        ("a-token-refresh", vec![Answer::granted("a-renewed-token", "a-renewed-refresh")]),
        ("a-renewed-refresh", vec![Answer::status(503, Vec::new())]),
    ];
    let github = StandIn::with(Script { renewals, ..Script::default() }).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_signed_in(dir.path(), &github);
    let telling = tokio::spawn(hub.tell_of_what_workers_are_sent());
    let mut feed = hub.subscribe_feed();
    let before = hub.store.hub_settings().unwrap();

    hub.check_github_logins(now_ms()).await;
    let renewed = (hub.store.hub_settings().unwrap(), sent(&hub));
    hub.check_github_logins(now_ms() + 9 * HOUR_MS).await;
    let told = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let HubNotice::HubSettings(settings) = std::sync::Arc::unwrap_or_clone(feed.recv().await.unwrap()) {
                return settings;
            }
        }
    });
    let told = told.await.expect("the settings were told of");
    telling.abort();

    assert_eq!(renewed, (before.clone(), as_ada("a-renewed-token")), "a renewal changes only the token");
    let Some(SentCredential::Blocked { why, reason, .. }) = told.github.as_ref().unwrap().sent.clone() else {
        panic!("{told:?}")
    };
    assert_eq!(why, UnusableLogin::Renewing);
    assert_eq!(GithubCredential::Blocked { reason }, sent(&hub), "with the reason workers are given");
    assert!(told.rev > before.rev);
    assert_eq!(hub.store.hub_settings().unwrap(), told);
}
