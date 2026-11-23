//! Telling a worker to watch as workers go, and as one doesn't answer.

use std::sync::Arc;

use protocol::model::{LineCounts, Observed};
use protocol::worker::WorkerResponse;

use super::stand_in::{Watch, frame, next_changes, next_lines, summary, until_watches, w, worker};
use crate::hub::observations::{WorkspaceChanges, WorkspaceLines};
use crate::hub::stand_in_workers::TIMEOUT;
use crate::hub::tests::hub_with_worktree;
use crate::hub::{Followed, Hub};
use crate::store::now_ms;
use crate::worker_link::{ANSWERED_WITHIN, WorkerLink};

/// Has the hub take in that `link`, a worker of `w`, has gone, as it does once the worker's stream ends.
async fn gone(hub: &Hub, link: &Arc<WorkerLink>) {
    let workspace = hub.workspace(&w()).await.unwrap();
    let provider = hub.provider(&workspace.provider).unwrap();
    hub.worker_gone(&w(), link, provider.as_ref(), &workspace.instance_id).await;
}

/// Once the worker watching a followed workspace's worktree has gone, its lines and its changed files are as of then,
/// both, since nothing watches them.
#[tokio::test]
async fn a_watched_workspace_s_changes_are_as_of_when_its_worker_went() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (link, asked) = worker(&hub, Watch::Watching);
    let _following = hub.follow([Followed::Workspace(w())]);
    until_watches(&asked, 1).await;
    hub.changes_frame(&w(), &link, frame(summary(3, 1))).await;
    let mut feed = hub.subscribe_feed();
    let before = now_ms();

    gone(&hub, &link).await;
    let lines = next_lines(&mut feed).await;
    let changes = next_changes(&mut feed).await;

    let Observed::AsOf { value, as_of_ms } = lines else { panic!("not as of a time: {lines:?}") };
    assert_eq!(value, LineCounts { added: 3, removed: 1 });
    assert!((before..=now_ms()).contains(&as_of_ms), "as of {as_of_ms}, gone after {before}");
    assert_eq!(changes, Observed::AsOf { value: summary(3, 1), as_of_ms }, "as of when the lines are");
}

/// A worker that takes a watch and never answers holds its workspace's telling only until the limit, which then goes
/// as a watch that failed does. Meanwhile the worker before it can go, without keeping the next worker from starting.
#[tokio::test(start_paused = true)]
#[expect(clippy::disallowed_methods, reason = "tokio's clock is paused, so sleeping moves it to before the limit")]
async fn a_worker_that_never_answers_a_watch_holds_its_telling_only_until_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let before = WorkerLink::answering(|_| Some(Ok(WorkerResponse::Ok)));
    let (_stuck, asked) = worker(&hub, Watch::Stuck);
    let _following = hub.follow([Followed::Workspace(w())]);
    until_watches(&asked, 1).await;

    let going = tokio::spawn({
        let hub = hub.clone();
        async move { gone(&hub, &before).await }
    });
    // The paused clock moves only once every task waits, so the worker before waits for its turn to be told by then.
    tokio::time::sleep(ANSWERED_WITHIN / 2).await;
    let gate_free = hub.worker_gate(&w()).try_lock().is_ok();
    tokio::time::timeout(ANSWERED_WITHIN, going).await.expect("the worker before went").unwrap();

    let current = || {
        hub.observation_is_kept_current::<WorkspaceLines>(&w())
            || hub.observation_is_kept_current::<WorkspaceChanges>(&w())
    };
    waiting::task::until(TIMEOUT, "the telling to run out, and let both kinds go", || !current()).await;

    assert!(gate_free, "the worker before kept the gate a worker starts under while it waited");
    assert_eq!(hub.changes_watches.kept(), 0, "it kept a watch that failed");
}
