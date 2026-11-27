//! What watches each worktree's branches and changes, and how a request starts, replaces or stops one.

use std::path::Path;
use std::sync::Arc;

use protocol::client::ErrorCode;
use protocol::model::{BranchName, Head};
use protocol::worker::{Request as _, WatchBranches, WatchChanges, WatchChildBranches, WorkerFrame};
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinHandle};

use super::super::{Watches, add_child, remove, watch_branches, watch_changes};
use super::support::{Served, added, committed, ends, next_frame, served, watch};
use crate::branch_watch::Whose;
use crate::git::tests::run;
#[cfg(unix)]
use crate::serve::dialing::without_hub;
use crate::serve::{Outgoing, handle};
use crate::testing::given;

/// A watch of a worktree replaces the earlier one of it and no other, and stopping one, as removing it does, leaves
/// the rest.
#[tokio::test]
async fn a_watch_replaces_only_its_own_worktree_s_and_stopping_one_leaves_the_rest() {
    let mut watches = Watches::default();
    let (own, first, again, other) = (watch(), watch(), watch(), watch());

    watches.start("/w".into(), Whose::Own, own.clone());
    watches.start("/c/a".into(), Whose::Child, first.clone());
    watches.start("/c/b".into(), Whose::Child, other.clone());
    watches.start("/c/a".into(), Whose::Child, again.clone());
    watches.stop(Path::new("/c/b"));

    ends(&first, "replaced").await;
    ends(&other, "stopped").await;
    // A wait lets the runtime run every task woken before it, so one stopped along with those has ended by now too.
    assert!(!own.is_finished() && !again.is_finished(), "the rest go on");
    watches.stop_all();
    ends(&own, "of the workspace's own worktree").await;
    ends(&again, "that replaced another").await;
}

/// A removal refused for the work it would lose leaves the worktree there, so its branches go on being watched. Its
/// changes stop being watched all the same, since that comes before git removes anything, as Windows needs.
#[tokio::test]
async fn a_removal_that_s_refused_leaves_the_worktree_s_branches_watched() {
    let served = served().await;
    let state = &served.worker.state;
    let path = served.child();
    added(add_child(state, served.adding()).await);
    watch_branches(state, path.clone(), Whose::Child).await.unwrap();
    let (sent, _frames) = mpsc::channel(64);
    state.outbox.attach(sent, false).await;
    watch_changes(state, watching(&path, None)).await.unwrap();
    let changes = changes_watch(&served, &path).expect("its changes are watched");
    std::fs::write(Path::new(&path).join("uncommitted.txt"), "work\n").unwrap();

    let refused = remove(state, served.removing(&path, false)).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::NeedsForce);
    assert!(state.watches.lock().children.contains_key(Path::new(&path)), "its watch stopped");
    ends(&changes, "of its changes").await;
    assert!(changes_watch(&served, &path).is_none(), "its changes' watch is still held");
}

/// The request that has the worktree at `path` watched against `base_ref`.
fn watching(path: &str, base_ref: Option<&str>) -> WatchChanges {
    WatchChanges { path: path.to_string(), base_ref: base_ref.map(str::to_string), watching: true }
}

/// What's watching the changes of the worktree at `path`, if anything is.
fn changes_watch(served: &Served, path: &str) -> Option<AbortHandle> {
    served.worker.state.watches.lock().changes.get(Path::new(path)).map(JoinHandle::abort_handle)
}

/// The commit the next summary of the worktree at `worktree` is against, skipping any other frame.
async fn next_base(frames: &mut mpsc::Receiver<Outgoing>, worktree: &str) -> String {
    next_frame(frames, |frame| {
        let WorkerFrame::Changes { path, summary } = frame else { return None };
        (path == worktree).then(|| summary.base_commit.as_str().to_string())
    })
    .await
}

/// The hub has a worktree's changes watched while a client follows its workspace, through the worker's requests: the
/// watch sends the summary, a watch asked for again, with another base, takes the place of the first, and one asked to
/// stop does.
#[tokio::test]
async fn a_watch_of_changes_is_replaced_by_the_next_and_stops_when_the_hub_says() {
    let served = served().await;
    let worker = &served.worker;
    let (sent, mut frames) = mpsc::channel(64);
    worker.state.outbox.attach(sent, false).await;
    let ask = |request| handle(&worker.state, &worker.forwarders, Ok(request));
    let own = served.own.clone();
    run(Path::new(&own), &["commit", "-q", "--allow-empty", "-m", "work"]);
    let (head, main) = (run(Path::new(&own), &["rev-parse", "HEAD"]), run(Path::new(&own), &["rev-parse", "main"]));

    ask(watching(&own, None).into_request()).await.unwrap();
    let first = next_base(&mut frames, &own).await;
    let earlier = changes_watch(&served, &own).expect("its changes are watched");
    ask(watching(&own, Some("main")).into_request()).await.unwrap();
    let again = next_base(&mut frames, &own).await;
    ends(&earlier, "replaced").await;
    let later = changes_watch(&served, &own).expect("its changes are watched again");
    let stopping = WatchChanges { watching: false, ..watching(&own, None) };
    ask(stopping.into_request()).await.unwrap();

    assert_eq!((first, again), (head, main), "each summary against the base its watch was asked for");
    ends(&later, "asked to stop").await;
    assert!(worker.state.watches.lock().changes.is_empty(), "a watch is still held");
}

/// Through what the worker does with each request: another worktree is made beside the workspace's own, its watch
/// sends frames naming it while the watch of the workspace's own goes on, and removing it stops its watches, and only
/// its, and lets go of what was read of its changes.
#[tokio::test]
async fn a_child_worktree_is_made_watched_and_removed_through_the_worker_s_requests() {
    let served = served().await;
    let worker = &served.worker;
    let (sent, mut frames) = mpsc::channel(64);
    worker.state.outbox.attach(sent, false).await;
    let ask = |request| handle(&worker.state, &worker.forwarders, Ok(request));
    let (own, child) = (served.own.clone(), served.child());
    ask(WatchBranches { path: own.clone() }.into_request()).await.unwrap();

    added(ask(served.adding().into_request()).await);
    ask(WatchChildBranches { path: child.clone() }.into_request()).await.unwrap();
    ask(watching(&child, None).into_request()).await.unwrap();
    let changes = changes_watch(&served, &child).expect("its changes are watched");
    worker.state.changes.summarize(&given(), Path::new(&child), None).await.unwrap();
    let watched = next_frame(&mut frames, |frame| {
        let WorkerFrame::ChildBranches { path, branches } = frame else { return None };
        (*path == child).then(|| branches.head.clone())
    });
    assert_eq!(watched.await, Head::Branch { name: BranchName::from("alder/try") });
    committed(&mut frames, &child, Whose::Child).await;
    committed(&mut frames, &own, Whose::Own).await;

    ask(served.removing(&child, false).into_request()).await.unwrap();

    assert!(!Path::new(&child).exists());
    let moved_to = served.dir.path().join("ws").join("repo").join(".deleting");
    assert!(moved_to.exists(), "moved aside beside the workspaces, which every archive clears");
    ends(&changes, "of its changes").await;
    assert!(!worker.state.changes.keeps(Path::new(&child)), "what was read of its changes is kept");
    let left = worker.state.watches.lock();
    assert!(left.children.is_empty() && left.changes.is_empty(), "its watches are still held");
    assert!(left.own.is_some(), "the workspace's own goes on");
}

/// A stopped watch of a worktree's changes, once waited for, has let go of all it held, the OS's watch among it, as
/// removing the worktree needs on Windows.
#[tokio::test]
async fn a_stopped_changes_watch_has_let_go_of_what_it_held_once_waited_for() {
    let mut watches = Watches::default();
    let held = Arc::new(());
    let holding = held.clone();
    let watch = async move {
        std::future::pending::<()>().await;
        drop(holding);
    };
    watches.watch_changes("/w".into(), tokio::spawn(watch));
    tokio::task::yield_now().await;

    let stopped = watches.stop_changes(Path::new("/w")).expect("it was watched");
    stopped.ended().await;

    assert_eq!(Arc::strong_count(&held), 1, "the watch still holds what it held");
}

/// A watch of a worktree's changes is for the hub that asked, so it ends as soon as the worker carries on without that
/// hub, rather than at its next read, which can be minutes later, since the next hub says again what it wants.
#[cfg(unix)]
#[tokio::test]
async fn a_watch_ends_once_its_hub_has_gone() {
    let served = served().await;
    let worker = &served.worker;
    let (sent, mut frames) = mpsc::channel(64);
    worker.state.outbox.attach(sent, false).await;
    watch_changes(&worker.state, watching(&served.own, None)).await.unwrap();
    let watch = changes_watch(&served, &served.own).expect("its changes are watched");
    // So the watch is waiting to hear something, rather than reading what it would send to a hub that's gone.
    next_base(&mut frames, &served.own).await;

    without_hub(worker).await;

    ends(&watch, "of a hub that's gone").await;
    assert!(worker.state.watches.lock().changes.is_empty(), "a watch is still held");
}
