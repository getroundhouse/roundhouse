//! The reads nobody asks for, as the hub's own loop starts them, through stand-ins for the workspaces' workers, on
//! tokio's clock, which moves on by itself while every task waits for it.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::fixtures::{pull_request_wait, workspace};
use protocol::model::{CheckState, PullRequestsSnapshot, WaitEnd, WaitId, WorkspaceId};
use protocol::worker::{
    PullRequestReply, PullRequestWaitUntil, RemotePullRequestsReply, StartPullRequestWait, WorkerRequest,
    WorkerResponse,
};
use tokio::time::Instant;

use super::scheduler::IN_TURN_RUNS;
use crate::hub::Hub;
use crate::hub::tests::hub_with_worktree;
use crate::store::Seen;
use crate::store::tests::idle_session;
use crate::worker_link::WorkerLink;

/// When a stand-in was asked what it never answers, each time.
type Asked = Arc<Mutex<Vec<Instant>>>;

/// Waits until `done`, failing rather than hanging if it never is.
async fn until(what: &str, done: impl Fn() -> bool) {
    waiting::task::until(Duration::from_mins(60), format!("{what} never happened"), done).await;
}

fn v() -> WorkspaceId {
    WorkspaceId::from("v")
}

/// A hub with the workspaces `w` and `v`, and a wait in `w` on #7's required checks, which pass at its next read. `w`'s
/// worker reads the waits' pull requests, finds no pull request of the workspace's own, and is kept, as while its chat
/// is open, so reading doesn't stop it. `v` has no worker yet.
async fn waiting_in_w(dir: &std::path::Path) -> (Arc<Hub>, WaitId) {
    let hub = hub_with_worktree(dir);
    hub.store.insert_workspace(&workspace("v"), None).unwrap();
    hub.store.workspace_ready(&v(), "/v", &"birch".into(), "main", None).unwrap();
    let found = Arc::new(Mutex::new(pull_request_wait("a").watch.unwrap().snapshot));
    let reading = reading_waits(&found);
    reading.keep();
    hub.workers.lock().insert("w".into(), reading.clone());
    let asking = StartPullRequestWait {
        pull_requests: pull_request_wait("a").watch.unwrap().requested,
        until: PullRequestWaitUntil::RequiredChecks,
        timeout_seconds: None,
        call: Some("toolu_1".into()),
    };
    let wait = hub.start_remote_wait(&"w".into(), &reading, "s".into(), asking).await.unwrap().wait.id;
    found.lock().pull_requests[0].checks[0].state = CheckState::Passed;
    (hub, wait)
}

/// A stand-in for a worker that reads the waits' pull requests as `found` holds then, finds no pull request of its
/// workspace's own, and does whatever else it's asked without a word.
fn reading_waits(found: &Arc<Mutex<PullRequestsSnapshot>>) -> Arc<WorkerLink> {
    let found = found.clone();
    WorkerLink::answering(move |request| {
        Some(Ok(if matches!(request, WorkerRequest::ReadRemotePullRequests(_)) {
            let observation = Ok(found.lock().clone());
            WorkerResponse::RemotePullRequests(RemotePullRequestsReply {
                observation,
                rate_limit: None,
                cost: 4,
                affected: Vec::new(),
            })
        } else if matches!(request, WorkerRequest::PullRequest(_)) {
            WorkerResponse::PullRequest(PullRequestReply { pull_request: None, branch: None })
        } else {
            WorkerResponse::Ok
        }))
    })
}

/// `v`'s worker, which never answers what `never` picks, as one stuck in `gh` doesn't, says when it was asked in
/// `asked`, and does whatever else it's asked without a word.
fn stuck_in_v(hub: &Hub, never: fn(&WorkerRequest) -> bool, asked: &Asked) -> Arc<WorkerLink> {
    let asked = asked.clone();
    let link = WorkerLink::answering(move |request| {
        if !never(&request) {
            return Some(Ok(WorkerResponse::Ok));
        }
        asked.lock().push(Instant::now());
        None
    });
    hub.workers.lock().insert(v(), link.clone());
    link
}

fn its_own_read(request: &WorkerRequest) -> bool {
    matches!(request, WorkerRequest::PullRequest(_))
}

fn a_read_of_waits(request: &WorkerRequest) -> bool {
    matches!(request, WorkerRequest::ReadRemotePullRequests(_))
}

fn ended(hub: &Hub, wait: &WaitId) -> Option<WaitEnd> {
    hub.store.wait(wait).unwrap().ended.map(|ended| ended.how)
}

#[tokio::test(start_paused = true)]
async fn a_read_in_turn_whose_worker_never_answers_holds_up_no_wait_and_is_stopped_at_its_limit() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, wait) = waiting_in_w(dir.path()).await;
    let asked = Asked::default();
    stuck_in_v(&hub, its_own_read, &asked);
    let limits = hub.read_limits(&v(), IN_TURN_RUNS).await;
    // As while a client is connected, so every workspace's pull request is read in turn.
    hub.clients.store(1, Ordering::Relaxed);

    let watching = tokio::spawn(hub.clone().watch_pull_requests());
    until("a read of v's pull request", || !asked.lock().is_empty()).await;
    let hung_at = asked.lock()[0];
    until("the wait ending", || ended(&hub, &wait).is_some()).await;
    let v_still_reading = hub.jobs.busy().contains(&v());
    until("v's worker being let go of", || hub.workers.lock().get(&v()).is_none()).await;
    let let_go_after = hung_at.elapsed();
    watching.abort();

    assert_eq!(ended(&hub, &wait), Some(WaitEnd::ChecksPassed));
    assert!(v_still_reading, "while v's read hung");
    assert!(let_go_after <= limits.work + limits.after, "within its limits, after {let_go_after:?}");
    assert!(hub.workers.lock().get(&"w".into()).is_some(), "w's worker, kept, still runs");
}

#[tokio::test(start_paused = true)]
async fn a_read_of_waits_whose_worker_never_answers_holds_up_no_other_workspaces_and_says_so_on_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, wait) = waiting_in_w(dir.path()).await;
    let asked = Asked::default();
    // Kept, so it's there to be asked again.
    stuck_in_v(&hub, a_read_of_waits, &asked).keep();
    let mut chat = idle_session(1);
    (chat.session.id, chat.session.workspace_id) = ("t".into(), v());
    hub.store.insert_session(&chat, None).unwrap();
    let theirs = protocol::model::Wait { workspace_id: v(), session_id: "t".into(), ..pull_request_wait("b") };
    hub.store.start_wait(&theirs, Seen::default()).unwrap();
    let read_error = |hub: &Hub| hub.store.wait(&theirs.id).unwrap().watch.unwrap().read_error;
    // No client is connected, so only waits' pull requests are read.

    let watching = tokio::spawn(hub.clone().watch_pull_requests());
    until("a read of v's waits", || !asked.lock().is_empty()).await;
    until("w's wait ending", || ended(&hub, &wait).is_some()).await;
    let v_still_reading = hub.jobs.busy().contains(&v());
    until("v's wait saying it wasn't read", || read_error(&hub).is_some()).await;
    let said_at = Instant::now();
    until("v's waits being read again", || asked.lock().len() > 1).await;
    let read_again_after = asked.lock()[1] - said_at;
    watching.abort();

    assert_eq!(ended(&hub, &wait), Some(WaitEnd::ChecksPassed));
    assert!(v_still_reading, "while v's read hung");
    assert_eq!(ended(&hub, &theirs.id), None, "v's wait goes on waiting");
    // Twice the two minutes between reads that worked.
    assert!(read_again_after > Duration::from_mins(3), "backing off, after {read_again_after:?}");
}

#[tokio::test(start_paused = true)]
async fn a_read_in_turn_that_doesnt_finish_waits_longer_each_time() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    hub.store.insert_workspace(&workspace("v"), None).unwrap();
    hub.store.workspace_ready(&v(), "/v", &"birch".into(), "main", None).unwrap();
    let asked = Asked::default();
    // Kept, so it's there to be asked again.
    stuck_in_v(&hub, its_own_read, &asked).keep();
    hub.clients.store(1, Ordering::Relaxed);

    let watching = tokio::spawn(hub.clone().watch_pull_requests());
    until("three reads of v's pull request", || asked.lock().len() > 2).await;
    watching.abort();

    let asked = asked.lock();
    let (first, second) = (asked[1] - asked[0], asked[2] - asked[1]);
    assert!(second > first + Duration::from_mins(2), "{first:?}, then {second:?}");
}
