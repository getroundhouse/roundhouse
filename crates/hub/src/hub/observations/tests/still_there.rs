//! The store keeps what's said of something only while the record it goes with is there, as the database says as the
//! change takes its revision: nothing of a workspace that's been removed, nor of an archived workspace's worktree,
//! which is back once it's restored. A GitHub account's is in `crate::hub::github_accounts`' tests.

use protocol::model::{OwnPullRequestSaid, WorkspaceId};
use rusqlite::Connection;

use super::super::kinds::{WorkspaceChanges, WorkspaceLines, WorkspacePullRequest};
use super::super::workspace_kinds::WorkspaceBranches;
use super::seen::{changes, lines, told};
use crate::hub::Hub;
use crate::hub::observations::Keeper;
use crate::hub::tests::{archived, hub_in, hub_in_again, hub_with_worktree};
use crate::store::{Ended, Move};
use crate::worker_link::WorkerLink;

/// Branches on `main`, as a worktree's worker says them.
fn on_main() -> protocol::model::Branches {
    protocol::model::Branches { head: protocol::model::Head::Detached { commit: "c1".into() }, had: Vec::new() }
}

/// What the worker of `w`'s worktree says of it, as it watches it: its lines, its changed files and its branches.
async fn its_worker_says(hub: &Hub) {
    let w = WorkspaceId::from("w");
    hub.observation_read::<WorkspaceLines>(w.clone(), lines(3, 1), 1_000).await.result().unwrap();
    hub.observation_read::<WorkspaceChanges>(w.clone(), changes(3, 1), 1_000).await.result().unwrap();
    hub.observation_read::<WorkspaceBranches>(w, on_main(), 1_000).await.result().unwrap();
}

/// Which of `w`'s worktree's kinds the hub keeps: its lines, its changed files and its branches.
fn its_worktree_s(hub: &Hub) -> [bool; 3] {
    let w = WorkspaceId::from("w");
    [
        hub.observations.keeps::<WorkspaceLines>(&w),
        hub.observations.keeps::<WorkspaceChanges>(&w),
        hub.observations.keeps::<WorkspaceBranches>(&w),
    ]
}

/// What GitHub says of `w`'s pull request, as the hub works it out.
async fn github_says(hub: &Hub) {
    let said = OwnPullRequestSaid { pull_request: None };
    hub.observation_worked_out::<WorkspacePullRequest>("w".into(), said, 1_000).await.result().unwrap();
}

/// A word that lands once its workspace was removed, of any kind, keeps nothing and tells nobody, whoever said it.
#[tokio::test]
async fn a_word_that_lands_after_its_workspace_was_removed_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let connection = Connection::open(dir.path().join("hub.db")).unwrap();
    connection.execute_batch("DELETE FROM sessions; DELETE FROM workspaces WHERE id = 'w';").unwrap();
    let mut feed = hub.subscribe_feed();

    its_worker_says(&hub).await;
    github_says(&hub).await;

    assert_eq!(its_worktree_s(&hub), [false; 3]);
    assert!(!hub.observations.keeps::<WorkspacePullRequest>(&"w".into()), "nor what GitHub says of it");
    assert_eq!(told(&mut feed), []);
}

/// What the worker of an archived workspace says of its worktree, which is gone, keeps nothing, while what GitHub says
/// of the workspace, which Home draws, is kept.
#[tokio::test]
async fn an_archived_workspace_s_worktree_is_said_nothing_of_while_github_s_word_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    archived(&hub);

    its_worker_says(&hub).await;
    github_says(&hub).await;

    assert_eq!(its_worktree_s(&hub), [false; 3]);
    assert!(hub.observations.keeps::<WorkspacePullRequest>(&"w".into()), "what GitHub says of it");
}

/// Once the database says an archived workspace is restored, or on its way back, its worker's word of its worktree is
/// kept again, as it says its branches and its changes are read.
#[tokio::test]
async fn a_restored_workspace_s_worktree_is_kept_again() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    archived(&hub);
    hub.store.start_move(&"w".into(), Move::Restore).unwrap().unwrap();

    hub.observation_read::<WorkspaceBranches>("w".into(), on_main(), 1_000).await.result().unwrap();
    let restoring = hub.observations.keeps::<WorkspaceBranches>(&"w".into());
    hub.store.end_move(&"w".into(), Move::Restore, Ended::Done).unwrap();
    its_worker_says(&hub).await;

    assert!(restoring, "on its way back");
    assert_eq!(its_worktree_s(&hub), [true; 3]);
}

/// What keeps an observation goes with the record it goes with, even where it hasn't said anything yet, as a worker
/// watching a worktree whose first read hasn't landed, so nothing is left to keep it.
#[tokio::test]
async fn a_keeper_that_said_nothing_goes_with_its_record() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let worker = Keeper::Worker(WorkerLink::unconnected().id);
    hub.observation_kept_by::<WorkspaceLines>("w".into(), worker).await.result().unwrap();
    let kept_current = hub.observation_is_kept_current::<WorkspaceLines>(&"w".into());

    hub.workspace_observations_gone(&"w".into()).await;

    assert!(kept_current, "kept current by its worker");
    assert!(!hub.observation_is_kept_current::<WorkspaceLines>(&"w".into()), "gone with its workspace");
}

/// A hub that starts on a database that says a workspace is archived forgets what its cache file kept of its
/// worktree, as one that stopped before forgetting it leaves, and keeps what GitHub says of it.
#[tokio::test]
async fn a_hub_that_starts_forgets_an_archived_workspace_s_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    its_worker_says(&hub).await;
    github_says(&hub).await;
    hub.observations_written().await;
    drop(hub);
    archived_in(&dir);

    let again = hub_in_again(dir.path());

    assert_eq!(its_worktree_s(&again), [false; 3]);
    assert!(again.observations.keeps::<WorkspacePullRequest>(&"w".into()), "what GitHub says of it");
}

/// Has the database in `dir` say `w` is archived, as a hub that stopped once it had recorded that leaves it.
fn archived_in(dir: &tempfile::TempDir) {
    let connection = Connection::open(dir.path().join("hub.db")).unwrap();
    connection.execute("UPDATE workspaces SET status = 'archived' WHERE id = 'w'", []).unwrap();
}
