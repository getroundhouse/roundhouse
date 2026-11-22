//! What's known of a workspace's own pull request between reads, from the cache: what a read on its way finds for
//! the asks that wait for it, what an ask just after a read takes, and what's known once the cache has forgotten it.

use std::time::Duration;

use github::PullRequestState;
use protocol::model::PrState;

use super::reading::{CLIENTS_TAKE_A_READ, known, numbered, queries};
use crate::hub::Hub;
use crate::hub::tests::{forget_github, hub_on_github, hub_on_github_timed, pushed};
use crate::timers::Timers;

/// Asked for again while a read is on its way, the next read is still needed, since the first began before the ask.
/// But a third, asked for while the second waits, has nothing to add once the second has read, so it shares its answer.
#[tokio::test]
async fn a_read_asked_for_while_another_waits_shares_its_answer() {
    assert_eq!(reads_of_three_asks_while_one_is_held(Timers::FOR_TESTS).await, 2, "the third shared the second's read");
}

/// A client's two asks made while a read is on its way, as from two clients, both take what it found, which began
/// within the time a client's ask takes a read.
#[tokio::test]
async fn asks_made_while_a_read_is_on_its_way_take_what_it_finds() {
    assert_eq!(reads_of_three_asks_while_one_is_held(CLIENTS_TAKE_A_READ).await, 1, "the others shared the first's");
}

/// A client's ask just after the branch was pushed again reads GitHub, rather than take what a read from before the
/// push found, however recent, since it found another head.
#[tokio::test]
async fn an_ask_after_a_push_reads_again_rather_than_take_a_read_from_before() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github_timed(dir.path(), CLIENTS_TAKE_A_READ).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.workspace_pull_request("w".into()).await.unwrap();
    let a_read = queries(&github).len();
    hub.workspace_pull_request("w".into()).await.unwrap();
    assert_eq!(queries(&github).len(), a_read, "an ask just after a read takes what it found");

    hub.store.set_workspace_branches(&"w".into(), &pushed("pushed", "pushed", (0, 0))).unwrap();
    hub.workspace_pull_request("w".into()).await.unwrap();

    assert!(queries(&github).len() > a_read, "read again after the push");
}

/// How many reads three asks for the workspace's pull request make, the second and third made while the first's read
/// is held on its way, in reads of it, with `timers`, after a read of it that's as old as a client's ask takes.
async fn reads_of_three_asks_while_one_is_held(timers: Timers) -> usize {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github_timed(dir.path(), timers).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.workspace_pull_request("w".into()).await.unwrap();
    let a_read = queries(&github).len();
    hub.github_cache.age(timers.fresh_for_clients);
    pull_requests.hold();
    let ask = || {
        let hub = hub.clone();
        tokio::spawn(async move { hub.workspace_pull_request("w".into()).await.unwrap() })
    };

    let first = ask();
    waiting::task::until(Duration::from_secs(30), "the first to reach GitHub", || queries(&github).len() > a_read)
        .await;
    let (second, third) = (ask(), ask());
    // Each asks before this test goes on, on a runtime of one thread, and waits for the first.
    tokio::task::yield_now().await;
    pull_requests.release();
    let answers = [first.await.unwrap(), second.await.unwrap(), third.await.unwrap()];

    assert!(answers.iter().all(|pr| pr.as_ref().map(|pr| numbered(pr).0) == Some(7)), "{answers:?}");
    assert_eq!(queries(&github).len() % a_read, 0, "whole reads");
    queries(&github).len() / a_read - 1
}

/// Once the cache has forgotten what GitHub said of a workspace's merged pull request, as after it was emptied, the
/// record still links to it, it's known as not looked at, and it's read again in turn, which it wasn't while it was
/// known to be merged. Each is as a hub that has just started finds it, with no read of it yet.
#[tokio::test]
async fn a_pull_request_the_cache_forgot_is_not_known_until_its_read_again() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Merged);
    hub.workspace_pull_request("w".into()).await.unwrap();
    hub.clients.store(1, std::sync::atomic::Ordering::Relaxed);
    let due = |hub: &std::sync::Arc<Hub>| {
        let hub = hub.clone();
        hub.pull_requests.schedules.lock().clear();
        async move { hub.to_read_in_turn().await.into_iter().map(|w| w.id.to_string()).collect::<Vec<_>>() }
    };
    assert_eq!(known(&hub).map(|pr| numbered(&pr).1), Some(PrState::Merged));
    assert_eq!(due(&hub).await, Vec::<String>::new(), "merged, so done");

    forget_github(&hub);

    let workspace = hub.store.get_workspace(&"w".into()).unwrap();
    assert_eq!(workspace.pull_request_linked().map(|link| link.number.get()), Some(7), "the link stays");
    assert_eq!(hub.known_of(&workspace), None, "not known");
    assert_eq!(due(&hub).await, ["w"], "so it's read again");
}
