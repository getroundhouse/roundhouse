use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode, HubNotice, ObservationNotice};
use protocol::model::{ChangeStatus, ChangeSummary, ChangedFile, LineCounts, Observation, Observed, WorkspaceId};
use protocol::worker::{WatchChanges, WorkerFrame, WorkerRequest, WorkerResponse};
use tokio::sync::broadcast;

use crate::hub::stand_in_workers::TIMEOUT;
use crate::hub::tests::{hub_with_worktree, serving};
use crate::hub::{Followed, FollowerId, Hub};
use crate::store::{Move, now_ms};
use crate::worker_link::WorkerLink;

fn by(subscription: u64) -> FollowerId {
    FollowerId { connection: 1, subscription }
}

fn w() -> WorkspaceId {
    "w".into()
}

/// A summary of one changed file that adds `added` lines and removes `removed`.
fn summary(added: u64, removed: u64) -> ChangeSummary {
    let lines = Some(LineCounts { added, removed });
    let file = ChangedFile { path: "a.rs".into(), from: None, status: ChangeStatus::Modified, lines, version: None };
    ChangeSummary::new("base".into(), vec![file])
}

/// Whether a stand-in worker watches, or refuses, as one from before the request does.
#[derive(Clone, Copy)]
enum Watch {
    Watching,
    Refusing,
}

/// A stand-in for `w`'s worker, serving it, that answers a watch as `watch` says and a summary with `summary(1, 0)`,
/// and keeps what it was asked.
fn worker(hub: &Hub, watch: Watch) -> (Arc<WorkerLink>, Arc<Mutex<Vec<WorkerRequest>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = asked.clone();
    let link = WorkerLink::answering(move |request| {
        let answer = if let WorkerRequest::WatchChanges(_) = request {
            match watch {
                Watch::Watching => Ok(WorkerResponse::Ok),
                Watch::Refusing => Err(ApiError::new(ErrorCode::Unsupported, "unknown request")),
            }
        } else if let WorkerRequest::SummarizeChanges(_) = request {
            Ok(WorkerResponse::ChangeSummary(summary(1, 0)))
        } else {
            Ok(WorkerResponse::Ok)
        };
        kept.lock().push(request);
        Some(answer)
    });
    serving(hub, &link);
    (link, asked)
}

/// Each watch the worker was asked for, in order.
fn watches(asked: &Mutex<Vec<WorkerRequest>>) -> Vec<WatchChanges> {
    let watch = |request: &WorkerRequest| {
        let WorkerRequest::WatchChanges(asked) = request else { return None };
        Some(asked.clone())
    };
    asked.lock().iter().filter_map(watch).collect()
}

fn summed_up(asked: &Mutex<Vec<WorkerRequest>>) -> usize {
    asked.lock().iter().filter(|request| matches!(request, WorkerRequest::SummarizeChanges(_))).count()
}

async fn until_watches(asked: &Mutex<Vec<WorkerRequest>>, count: usize) {
    waiting::task::until(TIMEOUT, format!("the worker to be told {count} times"), || watches(asked).len() == count)
        .await;
}

/// The lines of `w` as the feed tells of them next.
async fn next_lines(feed: &mut broadcast::Receiver<HubNotice>) -> Observed<LineCounts> {
    let next = async {
        loop {
            if let Ok(ObservationNotice::Copy(Observation::WorkspaceLines { lines, .. })) =
                ObservationNotice::try_from(feed.recv().await.unwrap())
            {
                return lines;
            }
        }
    };
    tokio::time::timeout(TIMEOUT, next).await.expect("the feed told of the lines")
}

/// The changed files of `w` as the feed tells of them next.
async fn next_changes(feed: &mut broadcast::Receiver<HubNotice>) -> Observed<ChangeSummary> {
    let next = async {
        loop {
            if let Ok(ObservationNotice::Copy(Observation::WorkspaceChanges { changes, .. })) =
                ObservationNotice::try_from(feed.recv().await.unwrap())
            {
                return changes;
            }
        }
    };
    tokio::time::timeout(TIMEOUT, next).await.expect("the feed told of the changed files")
}

fn frame(summary: ChangeSummary) -> WorkerFrame {
    WorkerFrame::Changes { path: "/w".into(), summary }
}

/// A workspace that a subscription follows has its running worker watch its worktree, so the lines and the changed
/// files it sends are current, both, and once nothing follows it the worker is told to stop, and both are as of then.
#[tokio::test]
async fn a_followed_workspace_s_worker_watches_its_worktree_until_nothing_follows_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (link, asked) = worker(&hub, Watch::Watching);
    let mut feed = hub.subscribe_feed();

    let following = hub.follow(by(1), [Followed::Workspace(w())]);
    let again = hub.follow(by(2), [Followed::Workspace(w())]);
    until_watches(&asked, 1).await;
    hub.changes_frame(&w(), &link, frame(summary(3, 1))).await;
    let current = next_lines(&mut feed).await;
    let changes_current = next_changes(&mut feed).await;
    drop(following);
    let still = hub.observation_is_kept_current::<crate::hub::observations::WorkspaceLines>(&w());
    let before = now_ms();
    drop(again);
    until_watches(&asked, 2).await;
    let let_go = next_lines(&mut feed).await;
    let changes_let_go = next_changes(&mut feed).await;

    let told = watches(&asked);
    assert_eq!(told[0], WatchChanges { path: "/w".into(), base_ref: Some("main".into()), watching: true });
    assert!(!told[1].watching);
    assert_eq!(current, Observed::Current { value: LineCounts { added: 3, removed: 1 } });
    assert!(still, "the second still follows it");
    let Observed::AsOf { value, as_of_ms } = let_go else { panic!("not as of a time: {let_go:?}") };
    assert_eq!(value, LineCounts { added: 3, removed: 1 });
    assert!((before..=now_ms()).contains(&as_of_ms), "as of {as_of_ms}, let go after {before}");
    assert_eq!(changes_current, Observed::Current { value: summary(3, 1) });
    assert_eq!(changes_let_go, Observed::AsOf { value: summary(3, 1), as_of_ms }, "let go with the lines");
}

/// A worker from before the request refuses it, and has the summary read once instead, as of the read, and isn't
/// asked again however many follow the workspace.
#[tokio::test]
async fn a_worker_from_before_the_watch_has_the_summary_read_once_instead() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (_link, asked) = worker(&hub, Watch::Refusing);
    let mut feed = hub.subscribe_feed();

    let _following = hub.follow(by(1), [Followed::Workspace(w())]);
    let lines = next_lines(&mut feed).await;
    let _again = hub.follow(by(2), [Followed::Workspace(w())]);
    hub.changes_watching(&w()).await;

    assert!(matches!(lines, Observed::AsOf { value: LineCounts { added: 1, removed: 0 }, .. }), "{lines:?}");
    assert_eq!((watches(&asked).len(), summed_up(&asked)), (1, 1));
}

/// The hub doesn't start a worker so that it can watch, as the owner decided, and tells the one it takes on later, as
/// a prompt's or a diff's starts one, and a replacement too.
#[tokio::test]
async fn only_a_worker_something_else_started_is_told_and_each_taken_on_is() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let _following = hub.follow(by(1), [Followed::Workspace(w())]);

    hub.changes_watching(&w()).await;

    assert!(hub.running_worker(&w()).is_none(), "a worker started for watching");
    let ready = hub.ready_workspace(&w()).await.unwrap();
    let (first, asked_first) = worker(&hub, Watch::Watching);
    hub.watch_branches(&ready, &first).await;
    until_watches(&asked_first, 1).await;
    let (second, asked_second) = worker(&hub, Watch::Watching);
    hub.watch_branches(&ready, &second).await;
    until_watches(&asked_second, 1).await;
}

/// What a worker that's been replaced still sends is dropped, and so is a summary of another worktree than the
/// workspace's own.
#[tokio::test]
async fn a_summary_from_a_replaced_worker_or_of_another_worktree_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (replaced, _) = worker(&hub, Watch::Watching);
    let (current, _) = worker(&hub, Watch::Watching);
    let mut feed = hub.subscribe_feed();

    hub.changes_frame(&w(), &replaced, frame(summary(9, 9))).await;
    let other = WorkerFrame::Changes { path: "/elsewhere".into(), summary: summary(8, 8) };
    hub.changes_frame(&w(), &current, other).await;
    hub.changes_frame(&w(), &current, frame(summary(2, 0))).await;

    let Observed::AsOf { value, .. } = next_lines(&mut feed).await else { panic!("not as of a read") };
    assert_eq!(value, LineCounts { added: 2, removed: 0 });
}

/// A workspace on its way to being archived has its worker told to stop before its worktree goes, and while its
/// worker watches, a turn's end reads nothing, since the watch sends what changes.
#[tokio::test]
async fn a_workspace_being_archived_stops_being_watched_and_a_watched_one_isn_t_read_at_a_turn_s_end() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (link, asked) = worker(&hub, Watch::Watching);
    let _following = hub.follow(by(1), [Followed::Workspace(w())]);
    until_watches(&asked, 1).await;

    hub.read_lines(&w(), &link);
    hub.store.start_move(&w(), Move::Archive).unwrap().unwrap();
    hub.changes_watching(&w()).await;

    assert_eq!(summed_up(&asked), 0, "read while watched");
    assert!(!watches(&asked)[1].watching);
}
