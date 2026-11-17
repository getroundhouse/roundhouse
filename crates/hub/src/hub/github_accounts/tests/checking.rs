//! The check by the wall clock, on tokio's paused clock, which doesn't count a Mac's sleep, as tokio's timers don't, so
//! a test moves the wall clock by as long as it likes and tokio's only to the next check. Each check here is
//! `GithubLogins::check` alone, since renewing calls GitHub, whose stand-in a paused clock races, as `crate::github`'s
//! device flow tests say, so what a renewal comes to is tested in `renewals`, on tokio's clock as it runs.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use protocol::model::{GithubToken, GithubUser, GithubUserId};
use protocol::worker::GithubCredential;
use tokio::sync::{Semaphore, mpsc};
use tokio::time::Instant;

use super::*;
use crate::github::TokenOwner;
use crate::github_logins::tests::{ada_signed_in, login_ending};
use crate::github_logins::{Connect, GithubLogins, RENEWED_WITH_LEFT};
use crate::store::tests::store_with_session;
use crate::store::{GithubProfile, Store};

const ENDS_AT_MS: i64 = 1_000_000_000;

const HOUR_MS: i64 = 60 * 60 * 1000;

/// The GitHub logins of a hub whose data directory is `dir`, started at 0, and its store.
fn logins_in(dir: &std::path::Path) -> (Arc<GithubLogins>, Arc<Store>) {
    let store = Arc::new(store_with_session(dir));
    (Arc::new(GithubLogins::new(Arc::clone(&store), dir, 0)), store)
}

fn sent(logins: &GithubLogins) -> GithubCredential {
    logins.git_credential().borrow().clone()
}

/// Checks `logins` at once, and every [`CHECKED_EVERY`] after, at what `wall_clock` says then, on a task of its own,
/// and sends what each check finds due, by what renews each token.
fn checking(logins: &Arc<GithubLogins>, wall_clock: &Arc<AtomicI64>) -> mpsc::UnboundedReceiver<Vec<String>> {
    let (found, checks) = mpsc::unbounded_channel();
    let (logins, wall_clock) = (Arc::clone(logins), Arc::clone(wall_clock));
    let check = move |now_ms| {
        let due = logins.check(now_ms).unwrap().due;
        found.send(due.iter().map(|due| due.refresh.as_str().to_string()).collect()).unwrap();
        std::future::ready(())
    };
    let wall_clock = move || wall_clock.load(Ordering::SeqCst);
    tokio::spawn(check_by_the_wall_clock(CHECKED_EVERY, wall_clock, std::future::pending(), check));
    checks
}

/// A Mac that sleeps for the eight hours its default's token has left, by the wall clock, while tokio's clock, which
/// doesn't count the sleep, moves only to the next check.
#[tokio::test(start_paused = true)]
async fn a_defaults_token_that_ends_while_the_hub_runs_blocks_at_the_next_check_by_the_wall_clock() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let connect = Connect::new(GithubAccountConnect { token: GithubToken::new("a-token"), id: None }, 0).unwrap();
    let user = GithubUser { id: GithubUserId::new(7), name: "ada".into() };
    logins
        .connect(&connect, &TokenOwner { user, profile: GithubProfile::default(), ends_at_ms: Some(ENDS_AT_MS) })
        .unwrap();
    let wall_clock = Arc::new(AtomicI64::new(ENDS_AT_MS - 8 * HOUR_MS));
    let mut checks = checking(&logins, &wall_clock);
    checks.recv().await.unwrap();
    let (as_the_hub_started, started) = (sent(&logins), Instant::now());

    wall_clock.store(ENDS_AT_MS, Ordering::SeqCst);
    checks.recv().await.unwrap();

    assert!(matches!(as_the_hub_started, GithubCredential::Account(_)), "the first check found it hadn't ended");
    assert_eq!(started.elapsed(), CHECKED_EVERY, "tokio's clock moved only to the next check");
    assert!(matches!(&sent(&logins), GithubCredential::Blocked { reason } if reason.contains("has ended")));
}

/// The wall clock moved to an hour before the sign-in's token ends, and then a millisecond past that, and tokio's
/// clock only to each next check.
#[tokio::test(start_paused = true)]
async fn a_sign_in_is_due_at_the_first_check_by_the_wall_clock_once_its_token_has_less_than_an_hour_left() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    ada_signed_in(&logins, &store, login_ending("a-token", ENDS_AT_MS));
    let an_hour = i64::try_from(RENEWED_WITH_LEFT.as_millis()).unwrap();
    let wall_clock = Arc::new(AtomicI64::new(ENDS_AT_MS - 8 * HOUR_MS));
    let mut checks = checking(&logins, &wall_clock);
    let as_the_hub_started = checks.recv().await.unwrap();
    let started = Instant::now();

    wall_clock.store(ENDS_AT_MS - an_hour, Ordering::SeqCst);
    let with_an_hour_left = checks.recv().await.unwrap();
    wall_clock.store(ENDS_AT_MS - an_hour + 1, Ordering::SeqCst);
    let with_less = checks.recv().await.unwrap();

    assert_eq!((as_the_hub_started, with_an_hour_left), (Vec::new(), Vec::new()));
    assert_eq!(with_less, ["a-token-refresh"]);
    assert_eq!(started.elapsed(), CHECKED_EVERY * 2, "tokio's clock moved only to each next check");
    assert!(matches!(sent(&logins), GithubCredential::Account(_)), "its token works still");
}

/// A Mac that sleeps from eight hours before the sign-in's token ends until two hours after, by the wall clock, while
/// tokio's clock moves only to the next check.
#[tokio::test(start_paused = true)]
async fn a_sign_in_slept_past_its_tokens_end_is_due_at_the_next_check_and_its_default_blocks_saying_its_renewing() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    ada_signed_in(&logins, &store, login_ending("a-token", ENDS_AT_MS));
    let wall_clock = Arc::new(AtomicI64::new(ENDS_AT_MS - 8 * HOUR_MS));
    let mut checks = checking(&logins, &wall_clock);
    checks.recv().await.unwrap();
    let started = Instant::now();

    wall_clock.store(ENDS_AT_MS + 2 * HOUR_MS, Ordering::SeqCst);
    let on_waking = checks.recv().await.unwrap();

    assert_eq!(on_waking, ["a-token-refresh"]);
    assert_eq!(started.elapsed(), CHECKED_EVERY, "tokio's clock moved only to the next check");
    let GithubCredential::Blocked { reason } = sent(&logins) else { panic!("{:?}", sent(&logins)) };
    assert!(reason.contains("has ended, and Roundhouse is renewing it"), "{reason}");
}

/// A check whose renewals GitHub is slow to answer, for three checks' time, so no check finds a sign-in due while it's
/// being renewed, and none is renewed twice at once.
#[tokio::test(start_paused = true)]
async fn a_check_starts_only_once_the_last_has_finished_and_those_missed_meanwhile_arent_caught_up_on() {
    let (started, mut starts) = mpsc::unbounded_channel();
    let finishing = Arc::new(Semaphore::new(0));
    let finish = Arc::clone(&finishing);
    tokio::spawn(check_by_the_wall_clock(
        CHECKED_EVERY,
        || 0,
        std::future::pending(),
        move |_| {
            started.send(()).unwrap();
            let finish = Arc::clone(&finish);
            async move { finish.acquire().await.unwrap().forget() }
        },
    ));
    starts.recv().await.unwrap();

    tokio::time::sleep(CHECKED_EVERY * 3).await;
    let while_the_first_ran = starts.try_recv();
    finishing.add_permits(1);
    let as_it_finished = tokio::time::timeout(Duration::from_millis(1), starts.recv()).await;
    finishing.add_permits(1);
    let before_a_check_more =
        tokio::time::timeout(CHECKED_EVERY.saturating_sub(Duration::from_millis(1)), starts.recv()).await;

    assert!(while_the_first_ran.is_err(), "no check started while the first ran");
    assert!(as_it_finished.is_ok(), "the next started as the first finished");
    assert!(before_a_check_more.is_err(), "and the one after only a check's time later");
}
