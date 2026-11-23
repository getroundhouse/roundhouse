//! Telling a worker to watch as subscriptions follow its workspace and stop, and what it sends.

use protocol::model::{LineCounts, Observed};
use protocol::worker::{WatchChanges, WorkerFrame};

use super::stand_in::{Watch, frame, next_changes, next_lines, summary, summed_up, until_watches, w, watches, worker};
use crate::hub::Followed;
use crate::hub::observations::WorkspaceLines;
use crate::hub::stand_in_workers::hub_with_stand_in_workers;
use crate::hub::tests::hub_with_worktree;
use crate::hub::workspace_lines;
use crate::store::now_ms;

/// A workspace that a subscription follows has its running worker watch its worktree, so the lines and the changed
/// files it sends are current, both, and once nothing follows it the worker is told to stop, and both are as of then.
/// The hub then keeps nothing of what it told.
#[tokio::test]
async fn a_followed_workspace_s_worker_watches_its_worktree_until_nothing_follows_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (link, asked) = worker(&hub, Watch::Watching);
    let mut feed = hub.subscribe_feed();

    let following = hub.follow([Followed::Workspace(w())]);
    let again = hub.follow([Followed::Workspace(w())]);
    until_watches(&asked, 1).await;
    hub.changes_frame(&w(), &link, frame(summary(3, 1))).await;
    let current = next_lines(&mut feed).await;
    let changes_current = next_changes(&mut feed).await;
    drop(following);
    // As the telling the first's going spawned does, which may not have run yet.
    hub.changes_watching(&w()).await;
    let still = (hub.observation_is_kept_current::<WorkspaceLines>(&w()), watches(&asked).len());
    let before = now_ms();
    drop(again);
    until_watches(&asked, 2).await;
    let let_go = next_lines(&mut feed).await;
    let changes_let_go = next_changes(&mut feed).await;

    let told = watches(&asked);
    assert_eq!(told[0], WatchChanges { path: "/w".into(), base_ref: Some("main".into()), watching: true });
    assert!(!told[1].watching);
    assert_eq!(current, Observed::Current { value: LineCounts { added: 3, removed: 1 } });
    assert_eq!(still, (true, 1), "the second still follows it");
    let Observed::AsOf { value, as_of_ms } = let_go else { panic!("not as of a time: {let_go:?}") };
    assert_eq!(value, LineCounts { added: 3, removed: 1 });
    assert!((before..=now_ms()).contains(&as_of_ms), "as of {as_of_ms}, let go after {before}");
    assert_eq!(changes_current, Observed::Current { value: summary(3, 1) });
    assert_eq!(changes_let_go, Observed::AsOf { value: summary(3, 1), as_of_ms }, "let go with the lines");
    assert_eq!(hub.changes_watches.kept(), 0, "it kept what it told a worker that watches nothing");
}

/// A worker from before the request refuses it, and has the summary read once instead, as of the read, and isn't
/// asked again however many follow the workspace, nor once they've all gone and one follows it again.
#[tokio::test]
async fn a_worker_from_before_the_watch_has_the_summary_read_once_instead() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (_link, asked) = worker(&hub, Watch::Refusing);
    let mut feed = hub.subscribe_feed();

    let following = hub.follow([Followed::Workspace(w())]);
    let lines = next_lines(&mut feed).await;
    let again = hub.follow([Followed::Workspace(w())]);
    hub.changes_watching(&w()).await;
    drop((following, again));
    hub.changes_watching(&w()).await;
    let _following = hub.follow([Followed::Workspace(w())]);
    hub.changes_watching(&w()).await;
    workspace_lines::tests::read(&hub, &w()).await;

    assert!(matches!(lines, Observed::AsOf { value: LineCounts { added: 1, removed: 0 }, .. }), "{lines:?}");
    assert_eq!((watches(&asked).len(), summed_up(&asked)), (1, 1));
}

/// The hub doesn't start a worker so that it can watch, as the owner decided, and tells the one it takes on later, as
/// a prompt's or a diff's starts one, and a replacement too.
#[tokio::test]
async fn only_a_worker_something_else_started_is_told_and_each_taken_on_is() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, started) = hub_with_stand_in_workers(dir.path());
    hub.store.workspace_ready(&w(), "/w", &"alder".into(), "main", None).unwrap();
    let _following = hub.follow([Followed::Workspace(w())]);

    hub.changes_watching(&w()).await;

    assert_eq!(started.workers_started(), 0, "a worker started for watching");
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

/// While its worker watches, a workspace's lines aren't read as a turn ends, since the watch sends what changes.
#[tokio::test]
async fn a_watched_workspace_isn_t_read_at_a_turn_s_end() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (link, asked) = worker(&hub, Watch::Watching);
    let _following = hub.follow([Followed::Workspace(w())]);
    until_watches(&asked, 1).await;

    hub.read_lines(&w(), &link);
    workspace_lines::tests::read(&hub, &w()).await;

    assert_eq!(summed_up(&asked), 0, "read while watched");
}
