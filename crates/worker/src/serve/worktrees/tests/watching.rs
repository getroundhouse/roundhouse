//! What watches each worktree's branches and changes, and how a request starts, replaces or stops one.

use std::path::Path;

use protocol::client::ErrorCode;
use protocol::model::{BranchName, Head};
use protocol::worker::{Request as _, WatchBranches, WatchChildBranches, WorkerFrame};
use tokio::sync::mpsc;

use super::super::{Watches, add_child, remove, watch_branches};
use super::support::{added, committed, ends, next_frame, served, watch};
use crate::branch_watch::Whose;
use crate::serve::handle;

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

/// A removal refused for the work it would lose leaves the worktree there, so its branches go on being watched.
#[tokio::test]
async fn a_removal_that_s_refused_leaves_the_worktree_watched() {
    let served = served().await;
    let state = &served.worker.state;
    let path = served.child();
    added(add_child(state, served.adding()).await);
    watch_branches(state, path.clone(), Whose::Child).await.unwrap();
    std::fs::write(Path::new(&path).join("uncommitted.txt"), "work\n").unwrap();

    let refused = remove(state, served.removing(&path, false)).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::NeedsForce);
    assert!(state.watches.lock().children.contains_key(Path::new(&path)), "its watch stopped");
}

/// Through what the worker does with each request: another worktree is made beside the workspace's own, its watch
/// sends frames naming it while the watch of the workspace's own goes on, and removing it stops its watch, and only
/// its.
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
    let left = worker.state.watches.lock();
    assert!(left.children.is_empty(), "its watch stopped");
    assert!(left.own.is_some(), "the workspace's own goes on");
}
