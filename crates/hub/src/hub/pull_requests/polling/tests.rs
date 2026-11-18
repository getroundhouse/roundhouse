//! The reads nobody asks for, as the hub's own loop starts them: workspaces' own pull requests and the waits' from a
//! stand-in for GitHub, on the real clock, since a call to it would race tokio's, and a read whose token never comes
//! from a stand-in for its worker, on tokio's clock, which moves on by itself while every task waits for it.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::fixtures::{pull_request_wait, workspace};
use protocol::model::{GhLogin, GhLoginWorks, WaitEnd, WaitId, WorkspaceId};
use protocol::worker::{FoundToken, GithubLoginReply, WorkerRequest, WorkerResponse};
use tokio::time::Instant;

use crate::hub::Hub;
use crate::hub::tests::{hub_on_github, hub_reading_github, hub_with_worktree, pushed};
use crate::store::tests::idle_session;
use crate::worker_link::WorkerLink;
use github::{Answer, PullRequestState, PullRequests, Script, StandIn};

/// When a stand-in was asked what it never answers, each time.
type Asked = Arc<Mutex<Vec<Instant>>>;

/// Waits until `done`, failing rather than hanging if it never is, on tokio's clock, which moves on by itself.
async fn until(what: &str, done: impl Fn() -> bool) {
    waiting::task::until(Duration::from_mins(60), format!("{what} never happened"), done).await;
}

/// The same, on the real clock, for the tests that call a stand-in for GitHub.
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
    hub.store.insert_workspace(&workspace("v"), None).unwrap();
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

/// `w`'s worker, which never answers what `never` picks, as one stuck doesn't, says when it was asked in `asked`, and
/// does whatever else it's asked without a word.
fn stuck_in_w(hub: &Hub, never: fn(&WorkerRequest) -> bool, asked: &Asked) -> Arc<WorkerLink> {
    let asked = asked.clone();
    let link = WorkerLink::answering(move |request| {
        if !never(&request) {
            return Some(Ok(WorkerResponse::Ok));
        }
        asked.lock().push(Instant::now());
        None
    });
    hub.workers.lock().insert("w".into(), link.clone());
    link
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
    hub.store.insert_workspace(&workspace("v"), None).unwrap();
    hub.store.workspace_ready(&v(), "/v", &"birch".into(), "main", None).unwrap();
    hub.store.set_workspace_branches(&v(), &pushed("head-7", "head-7", (0, 0))).unwrap();
    // As while a client is connected, so every workspace's pull request is read in turn.
    hub.clients.store(1, Ordering::Relaxed);
    let number = |id: &WorkspaceId| hub.store.get_workspace(id).unwrap().known_pull_request().map(|pr| pr.number);

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
        hub.store.insert_workspace(&workspace(id.as_str()), None).unwrap();
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

/// A machine's own `gh` that handed the hub no token, as one logged out as the hub started, then logged in, so its
/// workspace's worker is asked again, and the wait is read with what it hands over.
#[tokio::test]
async fn a_wait_read_as_a_machines_gh_that_handed_no_token_asks_its_worker_again() {
    let dir = tempfile::tempdir().unwrap();
    let users = vec![("b-token", Answer::user(1, "ada", None))];
    let script = Script { users, graphql: Answer::pull_request_read(7, true), ..Script::default() };
    let github = StandIn::with(script).await;
    let (hub, wait) = waiting_in_w(dir.path(), github.api());
    let provider = hub.store.get_workspace(&"w".into()).unwrap().provider;
    hub.found_tokens.found(provider.clone(), None);
    let link = WorkerLink::answering(|request| {
        if !matches!(request, WorkerRequest::FindGithubLogin) {
            return Some(Ok(WorkerResponse::Ok));
        }
        Some(Ok(WorkerResponse::GithubLogin(GithubLoginReply {
            login: GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::CouldntTell },
            token: Some(FoundToken { token: protocol::model::GithubToken::new("b-token"), renews: false }),
        })))
    });
    link.keep();
    hub.workers.lock().insert("w".into(), link);

    let watching = tokio::spawn(hub.clone().watch_pull_requests());
    soon("the wait ending", || ended(&hub, &wait).is_some()).await;
    watching.abort();

    assert_eq!(ended(&hub, &wait), Some(WaitEnd::ChecksPassed));
    let tokens: Vec<_> = github
        .asked()
        .into_iter()
        .filter(|asked| asked.starts_with("post /graphql "))
        .filter_map(|asked| {
            asked.lines().find_map(|line| line.strip_prefix("authorization: bearer ")).map(str::to_string)
        })
        .collect();
    assert_eq!(tokens, ["b-token", "b-token"]);
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

/// The machine's own `gh` handed the hub no token, and `w`'s worker never hands one over, as one stuck doesn't, so the
/// read is stopped at its limit, and each time it's read again later than the time before.
#[tokio::test(start_paused = true)]
async fn a_read_in_turn_that_doesnt_finish_waits_longer_each_time() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let address = protocol::model::RepoUrl::try_from("https://github.com/example/project".to_string()).unwrap();
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&address), None).unwrap();
    hub.store.set_workspace_branches(&"w".into(), &pushed("head-7", "head-7", (0, 0))).unwrap();
    let asked = Asked::default();
    // Kept, so it's there to be asked again.
    stuck_in_w(&hub, |request| matches!(request, WorkerRequest::FindGithubLogin), &asked).keep();
    hub.clients.store(1, Ordering::Relaxed);

    let watching = tokio::spawn(hub.clone().watch_pull_requests());
    until("three reads of w's pull request", || asked.lock().len() > 2).await;
    watching.abort();

    let asked = asked.lock();
    let (first, second) = (asked[1] - asked[0], asked[2] - asked[1]);
    assert!(second > first + Duration::from_mins(2), "{first:?}, then {second:?}");
}

/// `w`'s worker never hands over its machine's `gh`'s token, which the read in turn of `w` and `v` together waits for,
/// but holds neither's turn meanwhile, so a client asking after `v`, whose branch is pushed nowhere, so needs nothing
/// of GitHub, is answered at once, rather than once the read's limit cuts it off.
#[tokio::test(start_paused = true)]
async fn a_read_in_turn_waiting_for_a_token_holds_no_workspaces_turn() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let address = protocol::model::RepoUrl::try_from("https://github.com/example/project".to_string()).unwrap();
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&address), None).unwrap();
    hub.store.set_workspace_branches(&"w".into(), &pushed("head-7", "head-7", (0, 0))).unwrap();
    hub.store.insert_workspace(&workspace("v"), None).unwrap();
    hub.store.workspace_ready(&v(), "/v", &"birch".into(), "main", None).unwrap();
    let asked = Asked::default();
    stuck_in_w(&hub, |request| matches!(request, WorkerRequest::FindGithubLogin), &asked).keep();
    hub.clients.store(1, Ordering::Relaxed);

    let watching = tokio::spawn(hub.clone().watch_pull_requests());
    until("w's worker being asked for its token", || !asked.lock().is_empty()).await;
    let asking = Instant::now();
    let found = hub.workspace_pull_request(v()).await;
    let took = asking.elapsed();
    watching.abort();

    assert_eq!(found.unwrap(), None);
    assert!(took < Duration::from_secs(1), "{took:?}");
}

/// `w`'s worker never says whether its machine's `gh` has a token, which may yet come, so a client asking after its
/// pull request, with nothing known, is told it couldn't be read, rather than that it has none.
#[tokio::test(start_paused = true)]
async fn a_token_not_handed_over_yet_is_no_sign_that_gh_has_no_login() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let address = protocol::model::RepoUrl::try_from("https://github.com/example/project".to_string()).unwrap();
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&address), None).unwrap();
    hub.store.set_workspace_branches(&"w".into(), &pushed("head-7", "head-7", (0, 0))).unwrap();
    stuck_in_w(&hub, |request| matches!(request, WorkerRequest::FindGithubLogin), &Asked::default()).keep();

    let found = hub.workspace_pull_request("w".into()).await;

    assert!(found.is_err(), "{found:?}");
    assert_eq!(hub.store.get_workspace(&"w".into()).unwrap().pull_request, None);
}
