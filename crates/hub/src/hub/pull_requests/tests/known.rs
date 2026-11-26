//! What's known of a workspace's own pull request between reads, from the cache: what a read on its way finds for
//! the asks that wait for it, what an ask just after a read takes, and what's known once the cache has forgotten it.

use std::time::Duration;

use github::PullRequestState;
use protocol::client::ErrorCode;
use protocol::model::{GithubLoginKind, PrState, PullRequestLinked};

use super::super::reading::{Asked, When};
use super::reading::{CLIENTS_TAKE_A_READ, known, numbered, queries};
use crate::github_logins::DefaultCredential;
use crate::hub::tests::{archived, forget_github, hub_on_github, hub_on_github_timed, pushed};
use crate::hub::{Followed, Hub};
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

    hub.branches_known(&"w".into(), pushed("pushed", "pushed", (0, 0))).await;
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
    let _following = hub.follow([Followed::Hub]);
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

/// GitHub showing the default no repository to look in says nothing of whether the branch has a pull request, so the
/// hub no longer knows the none it found before, as on a branch it found none on before it was pushed, and keeps that
/// GitHub showed none, as whom and how its login is held, until a read sees the repository again.
#[tokio::test]
async fn a_repository_github_doesnt_show_leaves_the_pull_request_unknown_until_it_does() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    pull_requests.hide(true);
    hub.store.set_workspace_pull_request(&"w".into(), Some(&PullRequestLinked { link: None }), None).unwrap();

    let unread = hub.workspace_pull_request("w".into()).await;
    hub.observe_reads_ended().await;

    assert_eq!(unread.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(hub.store.get_workspace(&"w".into()).unwrap().pull_request_link, None, "none is no longer known");
    let unseen = hub.unseen_of(&"w".into()).unwrap();
    let who = (unseen.repository.as_str(), unseen.username.as_str(), unseen.login);
    assert_eq!(who, ("example/project", "ada", GithubLoginKind::Token));
    assert_eq!(unseen.approve_at, None, "a token's account has no app to approve");

    pull_requests.hide(false);
    let found = hub.workspace_pull_request("w".into()).await.unwrap();
    hub.observe_reads_ended().await;

    assert_eq!(found.map(|pr| numbered(&pr).0), Some(7));
    assert_eq!(hub.unseen_of(&"w".into()), None);
}

/// A read that fails after GitHub showed no repository, as without a login to read it as, says nothing new, so the hub
/// doesn't record none for want of anything known, which would have an app offer to create one there.
#[tokio::test]
async fn a_read_that_fails_after_github_showed_no_repository_records_no_none() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.hide(true);
    hub.workspace_pull_request("w".into()).await.unwrap_err();
    hub.observe_reads_ended().await;
    let DefaultCredential::Account { account, .. } = hub.git_credential().borrow().clone() else { panic!() };
    hub.github_account_disconnect(account).await.unwrap();

    let failed = hub.workspace_pull_request("w".into()).await;
    hub.observe_reads_ended().await;

    assert_eq!(failed.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(hub.store.get_workspace(&"w".into()).unwrap().pull_request_link, None, "not recorded as none");
    assert!(hub.unseen_of(&"w".into()).is_some(), "still unseen, as last heard");
}

/// A word of an archived workspace's branches is kept nowhere, and the read of its pull request it still has made, with
/// its branches forgotten as it was archived, asks GitHub nothing and records nothing.
#[tokio::test]
async fn an_archived_workspace_s_read_after_its_branches_moved_asks_and_records_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.branches_known(&"w".into(), pushed("local", "head-7", (2, 0))).await;
    hub.workspace_pull_request("w".into()).await.unwrap();
    let (asked, linked) = (queries(&github).len(), hub.store.get_workspace(&"w".into()).unwrap().pull_request_link);
    archived(&hub);
    hub.worktree_observations_gone(&"w".into()).await;

    // As the worker's word of its branches does once it's refused, where nothing was known before.
    hub.read_pull_request(&"w".into(), Asked::Agent, When::Always).await.unwrap();

    assert_eq!(queries(&github).len(), asked, "nothing asked of GitHub");
    assert_eq!(hub.store.get_workspace(&"w".into()).unwrap().pull_request_link, linked, "nothing recorded");
}

/// With its branches not known, as on the first start after they left the database with no worker running, which pull
/// request is the workspace's can't be told: a read asks GitHub nothing and records nothing, so one found before is
/// what's known, with the worktree as it stood against it then, and one not looked for yet isn't recorded as having
/// none, which would offer to make one.
#[tokio::test]
async fn with_its_branches_not_known_a_read_asks_and_records_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.branches_known(&"w".into(), pushed("local", "head-7", (2, 0))).await;
    hub.workspace_pull_request("w".into()).await.unwrap();
    hub.observe_github_once().await;
    let asked = queries(&github).len();
    hub.worktree_observations_gone(&"w".into()).await;

    let found_before = hub.workspace_pull_request("w".into()).await.unwrap();
    let linked_before = hub.store.get_workspace(&"w".into()).unwrap().pull_request_link;
    hub.store.set_workspace_pull_request(&"w".into(), None, None).unwrap();
    let not_looked_for = hub.workspace_pull_request("w".into()).await;

    assert_eq!(found_before.as_ref().map(numbered), Some((7, PrState::Open)), "as last read");
    assert_eq!(found_before.map(|own| own.local.ahead), Some(2), "as it stood then, not as nothing to push");
    assert_eq!(linked_before.and_then(|linked| linked.link).map(|link| link.number.get()), Some(7));
    assert_eq!(not_looked_for.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(hub.store.get_workspace(&"w".into()).unwrap().pull_request_link, None, "no none recorded");
    assert_eq!(queries(&github).len(), asked, "nothing asked of GitHub");
}
