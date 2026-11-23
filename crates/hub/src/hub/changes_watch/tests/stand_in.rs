//! A stand-in for `w`'s worker that watches, or refuses to, and what the tests read of what it was asked and of what
//! the hub's feed says.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode, HubNotice, ObservationNotice};
use protocol::model::WorkspaceId;
use protocol::model::{
    ChangeStatus, ChangeSummary, ChangedFile, GitFile, GitStatus, LineCounts, Observation, Observed,
};
use protocol::worker::{WatchChanges, WorkerFrame, WorkerRequest, WorkerResponse};
use tokio::sync::broadcast;

use crate::hub::stand_in_workers::TIMEOUT;
use crate::hub::tests::serving;
use crate::hub::{FollowerId, Hub};
use crate::worker_link::WorkerLink;

pub(super) fn by(subscription: u64) -> FollowerId {
    FollowerId { connection: 1, subscription }
}

pub(super) fn w() -> WorkspaceId {
    "w".into()
}

/// A summary of one changed file that adds `added` lines and removes `removed`.
pub(super) fn summary(added: u64, removed: u64) -> ChangeSummary {
    let lines = Some(LineCounts { added, removed });
    let file = ChangedFile { path: "a.rs".into(), from: None, status: ChangeStatus::Modified, lines, version: None };
    ChangeSummary::new("base".into(), vec![file])
}

/// How a stand-in worker answers a watch: it watches, refuses, as one from before the request does, or never answers,
/// as one that's stuck doesn't.
#[derive(Clone, Copy)]
pub(super) enum Watch {
    Watching,
    Refusing,
    Stuck,
}

/// A stand-in for `w`'s worker, serving it, that answers a watch as `watch` says, a summary with `summary(1, 0)`, and
/// a status with a change that isn't committed, and keeps what it was asked.
pub(super) fn worker(hub: &Hub, watch: Watch) -> (Arc<WorkerLink>, Arc<Mutex<Vec<WorkerRequest>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = asked.clone();
    let link = WorkerLink::answering(move |request| {
        let answer = match (&request, watch) {
            (WorkerRequest::WatchChanges(_), Watch::Refusing) => {
                Some(Err(ApiError::new(ErrorCode::Unsupported, "unknown request")))
            }
            (WorkerRequest::WatchChanges(_), Watch::Stuck) => None,
            (WorkerRequest::SummarizeChanges(_), _) => Some(Ok(WorkerResponse::ChangeSummary(summary(1, 0)))),
            (WorkerRequest::GitStatus(_), _) => Some(Ok(WorkerResponse::GitStatus(uncommitted()))),
            // A watch as `Watching` says, and everything else, is done.
            _ => Some(Ok(WorkerResponse::Ok)),
        };
        kept.lock().push(request);
        answer
    });
    serving(hub, &link);
    (link, asked)
}

/// A worktree's status with one change that isn't committed, which archiving would lose.
fn uncommitted() -> GitStatus {
    let file = GitFile { path: "a.rs".into(), orig_path: None, status: ".M".into() };
    GitStatus { files: vec![file], ..GitStatus::default() }
}

/// Each watch the worker was asked for, in order.
pub(super) fn watches(asked: &Mutex<Vec<WorkerRequest>>) -> Vec<WatchChanges> {
    let watch = |request: &WorkerRequest| {
        let WorkerRequest::WatchChanges(asked) = request else { return None };
        Some(asked.clone())
    };
    asked.lock().iter().filter_map(watch).collect()
}

pub(super) fn summed_up(asked: &Mutex<Vec<WorkerRequest>>) -> usize {
    asked.lock().iter().filter(|request| matches!(request, WorkerRequest::SummarizeChanges(_))).count()
}

pub(super) async fn until_watches(asked: &Mutex<Vec<WorkerRequest>>, count: usize) {
    waiting::task::until(TIMEOUT, format!("the worker to be told {count} times"), || watches(asked).len() == count)
        .await;
}

/// The lines of `w` as the feed tells of them next.
pub(super) async fn next_lines(feed: &mut broadcast::Receiver<HubNotice>) -> Observed<LineCounts> {
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
pub(super) async fn next_changes(feed: &mut broadcast::Receiver<HubNotice>) -> Observed<ChangeSummary> {
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

pub(super) fn frame(summary: ChangeSummary) -> WorkerFrame {
    WorkerFrame::Changes { path: "/w".into(), summary }
}
