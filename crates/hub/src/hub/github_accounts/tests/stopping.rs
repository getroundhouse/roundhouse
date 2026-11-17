//! The hub stopping as it checks its GitHub logins: while GitHub renews a sign-in, and after, against a stand-in for
//! GitHub on tokio's clock as it runs, since a request to the stand-in races a paused clock, and between checks, on
//! tokio's paused clock, as nothing there asks GitHub anything.

use std::sync::Arc;
use std::time::Duration;

use protocol::model::GithubToken;
use tokio::time::Instant;

use super::renewals::{hub_signed_in, renewing};
use crate::github::{Answer, Gate, GithubRefreshToken};
use crate::github_logins::tests::{ada_signed_in, login_ending, sign_in_held};
use crate::hub::tests::hub_in;
use crate::store::now_ms;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// GitHub holding back what it gives as it renews the sign-in, as it may for up to the call's 15 s, until the hub has
/// begun to stop.
#[tokio::test]
async fn a_stop_while_github_renews_a_sign_in_waits_for_its_answer_and_keeps_what_it_gave() {
    let gate = Gate::closed();
    let granted = Answer::granted("a-renewed-token", "a-renewed-refresh");
    let github = renewing(vec![Answer::After(gate.clone(), Box::new(granted))]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_signed_in(dir.path(), &github);
    let watching = tokio::spawn(Arc::clone(&hub).watch_github_logins());
    github.until_asked(1).await;

    let stopping = tokio::spawn({
        let hub = Arc::clone(&hub);
        async move { hub.stop_checking_github_logins(watching).await }
    });
    hub.github_checks_stopped.subscribe().wait_for(|stopped| *stopped).await.unwrap();
    gate.open();
    stopping.await.unwrap();

    let held = sign_in_held(&hub.github_logins, &hub.store, &"g".into());
    let renewed = (GithubToken::new("a-renewed-token"), Some(GithubRefreshToken::new("a-renewed-refresh")));
    assert_eq!((held.token, held.refresh), renewed, "the stop waited for GitHub's answer, and kept what it gave");
    assert_eq!(github.asked().len(), 1, "and no check started after");
}

/// A sign-in whose token, and what renews it, both ended days ago, which the first check has need signing in again
/// without asking GitHub, so the hub stops as it waits for the next check.
#[tokio::test(start_paused = true)]
async fn a_stop_between_checks_waits_for_no_check() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let account = ada_signed_in(&hub.github_logins, &hub.store, login_ending("a-token", now_ms() - 2 * DAY_MS));
    let watching = tokio::spawn(Arc::clone(&hub).watch_github_logins());
    tokio::time::sleep(hub.timers.check_github_logins_every / 2).await;
    let started = Instant::now();

    hub.stop_checking_github_logins(watching).await;

    assert_eq!(started.elapsed(), Duration::ZERO, "the stop waited for no check");
    let checked = hub.store.github_account(&account).unwrap().unwrap();
    assert!(checked.held_login.unwrap().needs_sign_in, "after the first had run");
}

/// A check under way as the hub stops, which finds the sign-in due after the stop.
#[tokio::test]
async fn once_the_hub_has_stopped_checking_a_check_starts_no_renewal() {
    let github = renewing(vec![Answer::granted("a-renewed-token", "a-renewed-refresh")]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_signed_in(dir.path(), &github);
    hub.stop_checking_github_logins(tokio::spawn(async {})).await;

    hub.check_github_logins(now_ms()).await;

    assert_eq!(github.asked(), Vec::<String>::new(), "GitHub was asked nothing");
    let held = sign_in_held(&hub.github_logins, &hub.store, &"g".into());
    assert_eq!(held.token, GithubToken::new("a-token"));
}
