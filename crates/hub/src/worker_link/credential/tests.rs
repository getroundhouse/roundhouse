//! Links following the hub's git credential, to stand-ins for workers, on a paused clock, so one that never answers
//! holds a test up only as long as it takes to move the clock.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::github_accounts::GithubAccountConnect;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GithubAccountId, GithubToken, GithubUser, GithubUserId};
use protocol::worker::{GitCredential, GitCredentialSet, GithubCredential, WorkerRequest, WorkerResponse};
use tokio::sync::watch;

use super::{Following, SET_WITHIN};
use crate::github::TokenOwner;
use crate::github_logins::{Connect, GithubLogins};
use crate::logged::Logs;
use crate::store::tests::store_with_session;
use crate::worker_link::WorkerLink;

/// A stand-in for a worker, following `credential` as an opened link does, which keeps each credential it's set, in
/// turn, and answers each as `answer` says, or never, for `None`.
fn following(
    credential: watch::Receiver<GithubCredential>,
    answer: impl Fn() -> Option<Result<WorkerResponse, ApiError>> + Send + 'static,
) -> (Arc<WorkerLink>, watch::Receiver<Vec<GithubCredential>>) {
    let (kept, sets) = watch::channel(Vec::new());
    let link = WorkerLink::answering(move |request| {
        let WorkerRequest::GitCredentialSet(GitCredentialSet { credential }) = request else { panic!("{request:?}") };
        kept.send_modify(|sets| sets.push(credential));
        answer()
    });
    tokio::spawn(Following::new(Arc::clone(&link), credential).follow());
    (link, sets)
}

/// Once a stand-in's sets are as `done` says, which comes before the clock moves, since nothing else is waited for, and
/// otherwise fails, once the clock has moved a second.
async fn until(
    sets: &mut watch::Receiver<Vec<GithubCredential>>,
    done: impl FnMut(&Vec<GithubCredential>) -> bool,
) -> Vec<GithubCredential> {
    let sets = tokio::time::timeout(Duration::from_secs(1), sets.wait_for(done)).await;
    sets.expect("the stand-in wasn't set what was waited for").unwrap().clone()
}

/// Once a stand-in has been set `count` credentials, as [`until`] says.
async fn until_set(sets: &mut watch::Receiver<Vec<GithubCredential>>, count: usize) -> Vec<GithubCredential> {
    until(sets, |sets| sets.len() >= count).await
}

/// The GitHub logins of a hub in `dir`, with none.
fn logins_in(dir: &Path) -> GithubLogins {
    GithubLogins::new(Arc::new(store_with_session(dir)), dir, 0)
}

/// Connects `token`, as ada's, renewing the account `renewing` if it's given one, and returns the account's ID.
fn connect(logins: &GithubLogins, token: &str, renewing: Option<&GithubAccountId>) -> GithubAccountId {
    let asked = GithubAccountConnect { token: GithubToken::new(token), id: renewing.cloned() };
    let user = GithubUser { id: GithubUserId::new(7), name: "ada".into() };
    let owner = TokenOwner { user, ends_at_ms: None };
    logins.connect(&Connect::new(asked, 0).unwrap(), &owner).unwrap().account.id
}

fn account(token: &str) -> GithubCredential {
    GithubCredential::Account(GitCredential { username: "ada".into(), token: GithubToken::new(token) })
}

/// A set at a time, as fast as the hub can make them, so a link that's applying one when the next comes is sent
/// only the newest after.
#[tokio::test(start_paused = true)]
async fn every_link_is_sent_each_change_in_the_order_made_ending_with_the_newest() {
    let made = [account("first"), GithubCredential::Blocked { reason: "R".into() }, account("second")];
    let credential = watch::Sender::new(GithubCredential::MachineOwn);
    let mut links: Vec<_> =
        (0..3).map(|_| following(credential.subscribe(), || Some(Ok(WorkerResponse::Ok)))).collect();

    for (at, set) in made.iter().enumerate() {
        credential.send_replace(set.clone());
        for _ in 0..at {
            tokio::task::yield_now().await;
        }
    }

    for (link, sets) in &mut links {
        let sets = until(sets, |sets| sets.last() == made.last()).await;
        let mut in_order = made.iter();
        assert!(sets.iter().all(|set| in_order.any(|made| made == set)), "{sets:?}");
        assert!(link.is_open());
    }
}

#[tokio::test(start_paused = true)]
async fn a_worker_that_never_answers_holds_up_no_other_link_nor_a_change_and_is_ended_at_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let logins = logins_in(dir.path());
    let (answering, mut answered) = following(logins.git_credential(), || Some(Ok(WorkerResponse::Ok)));
    let (stuck, mut stuck_sets) = following(logins.git_credential(), || None);
    let started = tokio::time::Instant::now();

    let id = connect(&logins, "a-token", None);
    until_set(&mut stuck_sets, 1).await;
    logins.disconnect(&id, 0).unwrap();
    let both = until_set(&mut answered, 2).await;
    let stuck_meanwhile = (stuck.is_open(), started.elapsed());
    let ended = tokio::time::timeout(SET_WITHIN * 2, stuck.wait_closed()).await;

    assert_eq!(both[0], account("a-token"));
    assert!(matches!(&both[1], GithubCredential::Blocked { .. }), "{both:?}");
    assert_eq!(stuck_meanwhile, (true, Duration::ZERO), "the other link had both while this one waited");
    let at = started.elapsed();
    assert!(ended.is_ok(), "it was never ended");
    assert!(at >= SET_WITHIN && at < SET_WITHIN + Duration::from_secs(1), "it was ended at {at:?}");
    assert!(answering.is_open());
}

/// A worker from before sets is said once a link, one that can't apply a set each time, with why, and both are used on.
/// Nothing logs a token, neither as what workers are sent changes, nor as a link takes an answer.
#[tokio::test(start_paused = true)]
async fn a_worker_that_doesnt_apply_a_set_is_said_and_used_on_and_no_token_is_logged() {
    let logs = Logs::capture();
    let dir = tempfile::tempdir().unwrap();
    let logins = logins_in(dir.path());
    let older = || Some(Err(ApiError::new(ErrorCode::Unsupported, "a request this worker doesn't know")));
    let (from_before, mut from_before_sets) = following(logins.git_credential(), older);
    let old_git = || Some(Err(ApiError::new(ErrorCode::Unavailable, "its git is older than 2.31")));
    let (cant, mut cant_sets) = following(logins.git_credential(), old_git);

    let id = connect(&logins, "github_pat_a_secret", None);
    until_set(&mut from_before_sets, 1).await;
    until_set(&mut cant_sets, 1).await;
    connect(&logins, "github_pat_another_secret", Some(&id));
    until_set(&mut from_before_sets, 2).await;
    until_set(&mut cant_sets, 2).await;

    let logged = logs.text();
    assert!(from_before.is_open() && cant.is_open());
    assert_eq!(logged.matches("doesn't take a git credential").count(), 1, "{logged}");
    assert_eq!(logged.matches("couldn't apply its git credential").count(), 2, "{logged}");
    assert!(logged.contains("its git is older than 2.31"), "{logged}");
    assert!(logged.contains("what workers give their processes for github.com from now on"), "{logged}");
    assert!(!logged.contains("secret"), "{logged}");
}
