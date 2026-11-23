//! The reads nobody asks for, as the hub's own loop starts them: workspaces' own pull requests and the waits' from a
//! stand-in for GitHub, on the real clock, since a call to it would race tokio's, and a read whose token never comes
//! from a stand-in for its worker, on tokio's clock, which moves on by itself while every task waits for it.

use std::sync::Arc;
use std::time::Duration;

use protocol::fixtures::{pull_request_wait, workspace};
use protocol::model::{WaitEnd, WaitId, WorkspaceId};
use tokio::time::Instant;

use crate::hub::tests::{hub_on_github, hub_reading_github, pushed};
use crate::hub::{Followed, Hub};
use crate::store::tests::idle_session;
use github::{Answer, PullRequestState, PullRequests, Script, StandIn};

/// Waits until `done`, on the real clock, for the tests that call a stand-in for GitHub.
async fn soon(what: &str, done: impl Fn() -> bool) {
    waiting::task::until(Duration::from_secs(30), format!("{what} never happened"), done).await;
}

fn v() -> WorkspaceId {
    WorkspaceId::from("v")
}

/// A hub reading GitHub at `api`, with the workspaces `w` and `v`, and a wait in `w` on #7's required checks. `v` has
/// no worker yet.
fn waiting_in_w(dir: &std::path::Path, api: String) -> (Arc<Hub>, WaitId) {
    let hub = hub_reading_github(dir, api);
    hub.store
        .insert_workspace(&workspace("v"), &crate::store::WorkspacePlacement::local_checkout("/repo"), None)
        .unwrap();
    hub.store.workspace_ready(&v(), "/v", &"birch".into(), "main", None).unwrap();
    let wait = hub.store.start_wait(&pull_request_wait("a")).unwrap().id;
    (hub, wait)
}

/// A wait in `v` too, by its chat `t`, on the same pull request.
fn waiting_in_v_too(hub: &Hub) -> WaitId {
    waiting_by(hub, v(), "t")
}

/// A wait in `workspace`, by its chat `chat`, on #7's required checks.
fn waiting_by(hub: &Hub, workspace: WorkspaceId, chat: &str) -> WaitId {
    let mut session = idle_session(1);
    (session.session.id, session.session.workspace_id) = (chat.into(), workspace.clone());
    hub.store.insert_session(&session, None).unwrap();
    let wait = protocol::model::Wait { workspace_id: workspace, session_id: chat.into(), ..pull_request_wait(chat) };
    hub.store.start_wait(&wait).unwrap().id
}

fn read_error(hub: &Hub, wait: &WaitId) -> Option<String> {
    hub.store.wait(wait).unwrap().watch.unwrap().read_error
}

fn ended(hub: &Hub, wait: &WaitId) -> Option<WaitEnd> {
    hub.store.wait(wait).unwrap().ended.map(|ended| ended.how)
}

/// `w` and `v` are on the same branch, which nothing stops, so both have its pull request, read as this machine's own
/// login in one read, whose queries ask after both, and with no worker started for it.
#[tokio::test]
async fn workspaces_due_as_one_token_are_read_together_without_a_worker() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.store
        .insert_workspace(&workspace("v"), &crate::store::WorkspacePlacement::local_checkout("/repo"), None)
        .unwrap();
    hub.store.workspace_ready(&v(), "/v", &"birch".into(), "main", None).unwrap();
    hub.store.set_workspace_branches(&v(), &pushed("head-7", "head-7", (0, 0))).unwrap();
    // As while a client follows the hub, so every workspace's pull request is read in turn.
    let _following = hub.follow([Followed::Hub]);
    let number =
        |id: &WorkspaceId| hub.store.get_workspace(id).unwrap().pull_request_linked().map(|link| link.number.get());

    let watching = tokio::spawn(hub.clone().watch_pull_requests());
    soon("both being read", || number(&"w".into()).is_some() && number(&v()).is_some()).await;
    watching.abort();

    assert_eq!((number(&"w".into()), number(&v())), (Some(7), Some(7)));
    let queries: Vec<_> = github.asked().into_iter().filter(|asked| asked.starts_with("post /graphql ")).collect();
    assert!(queries[0].contains("r0: repository(") && queries[0].contains("r1: repository("), "{queries:?}");
    assert_eq!(queries.len(), 3, "which branches have which, then the pull request's two rounds: {queries:?}");
    assert!(hub.workers.lock().is_empty(), "no worker was started to read them");
}

/// `w`'s and `v`'s waits are read as this machine's own login, so in one read, whose queries ask of both, and which
/// finds #7's checks have passed, so both end.
#[tokio::test]
async fn waits_due_as_one_token_are_read_together_and_end_as_the_read_finds() {
    let dir = tempfile::tempdir().unwrap();
    let github =
        StandIn::with(Script { graphql: Answer::pull_requests_read(&[(7, true), (7, true)]), ..Script::default() })
            .await;
    let (hub, wait) = waiting_in_w(dir.path(), github.api());
    let theirs = waiting_in_v_too(&hub);

    let watching = tokio::spawn(hub.clone().watch_pull_requests());
    soon("both waits ending", || ended(&hub, &wait).is_some() && ended(&hub, &theirs).is_some()).await;
    watching.abort();

    assert_eq!((ended(&hub, &wait), ended(&hub, &theirs)), (Some(WaitEnd::ChecksPassed), Some(WaitEnd::ChecksPassed)));
    let queries: Vec<_> = github.asked().into_iter().filter(|asked| asked.starts_with("post /graphql ")).collect();
    assert_eq!(queries.len(), 2, "{queries:?}");
    assert!(queries.iter().all(|query| query.contains("r0: repository(") && query.contains("r1: repository(")));
    assert!(hub.workers.lock().is_empty(), "no worker was started to read them");
}

/// More workspaces waiting than the reads nobody is waiting on may spend at once, each in a group of its own, as many as
/// a hundred open workspaces would bring, which are read together in a query or two rather than put off for good.
#[tokio::test]
async fn more_groups_of_waits_than_a_burst_of_reads_are_read_together_all_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let pull_requests = PullRequests::default();
    pull_requests.set(7, PullRequestState::Passed);
    let github = StandIn::with(Script { pull_requests: Some(pull_requests), ..Script::default() }).await;
    let (hub, wait) = waiting_in_w(dir.path(), github.api());
    let mut waits = vec![wait];
    for n in 0..40 {
        let id = WorkspaceId::from(format!("x{n}").as_str());
        hub.store
            .insert_workspace(&workspace(id.as_str()), &crate::store::WorkspacePlacement::local_checkout("/repo"), None)
            .unwrap();
        hub.store.workspace_ready(&id, &format!("/x{n}"), &format!("x{n}").into(), "main", None).unwrap();
        waits.push(waiting_by(&hub, id, &format!("c{n}")));
    }

    let watching = tokio::spawn(hub.clone().watch_pull_requests());
    soon("every wait ending", || waits.iter().all(|wait| ended(&hub, wait).is_some())).await;
    watching.abort();

    assert!(waits.iter().all(|wait| ended(&hub, wait) == Some(WaitEnd::ChecksPassed)));
    let queries = github.asked().into_iter().filter(|asked| asked.starts_with("post /graphql ")).count();
    assert!(queries <= 4, "{queries} queries");
}

/// GitHub failing, which reading again may get past, so the waits go on waiting, saying it, and are read again later
/// than they would have been.
#[tokio::test]
async fn a_read_of_waits_github_fails_says_so_on_them_and_backs_off() {
    let dir = tempfile::tempdir().unwrap();
    let github = StandIn::with(Script { graphql: vec![Answer::status(503, Vec::new())], ..Script::default() }).await;
    let (hub, wait) = waiting_in_w(dir.path(), github.api());

    let watching = tokio::spawn(hub.clone().watch_pull_requests());
    soon("the wait saying it wasn't read", || read_error(&hub, &wait).is_some()).await;
    // Which is scheduled as it's recorded.
    let schedule = || hub.pull_requests.schedules.lock().get(&WorkspaceId::from("w")).copied();
    soon("its next read being put off", || schedule().is_some_and(|schedule| !schedule.found)).await;
    watching.abort();

    assert_eq!(ended(&hub, &wait), None);
    let due_at = schedule().unwrap().due_at_soonest();
    assert!(due_at > Instant::now() + Duration::from_mins(3), "twice the two minutes between reads that worked");
}
