//! Making, removing and putting back a worktree, each in its turn, so a request heard again or asked meanwhile waits for
//! the one under way.

use std::path::Path;

use protocol::worker::{RestoreWorkspace, WorkerResponse, WorktreeThere, WorktreeThereReply};

use super::super::{add_child, remove, restore, there};
use super::support::{added, served};
use crate::serve::moves::tests::BOUND;

/// A new hub asks again for what the last didn't hear back about, which may still be being made.
#[tokio::test]
async fn an_add_heard_again_waits_for_one_under_way_and_answers_with_what_it_made() {
    let served = served().await;
    let state = &served.worker.state;
    let path = served.child();
    // Held as an add under way holds it, so whether the next waits doesn't turn on which git finishes first.
    let under_way = state.moves.alone(Path::new(&path)).await;
    let heard_again = add_child(state, served.adding());
    tokio::pin!(heard_again);
    let early = tokio::time::timeout(BOUND, &mut heard_again).await;
    assert!(early.is_err() && !Path::new(&path).exists(), "it didn't wait for the one under way");
    drop(under_way);
    let first = added(heard_again.await);

    let (second, third) = tokio::join!(add_child(state, served.adding()), add_child(state, served.adding()));

    let (second, third) = (added(second), added(third));
    assert_eq!((&second.commit, &third.commit), (&first.commit, &first.commit));
    let warned = [&second.warnings, &third.warnings];
    assert!(warned.iter().all(|warnings| warnings.is_empty()), "each found it made, rather than failing: {warned:?}");
}

/// A removal heard while the worktree is being made, as after a new hub finds it recorded as being made, removes what
/// the add makes, rather than finding nothing and leaving it.
#[tokio::test]
async fn a_removal_waits_for_an_add_under_way() {
    let served = served().await;
    let state = &served.worker.state;
    let path = served.child();

    // Joined, so the add is polled first and takes the path before the removal looks.
    let (made, removed) = tokio::join!(add_child(state, served.adding()), remove(state, served.removing(&path, true)));

    added(made);
    assert!(matches!(removed, Ok(WorkerResponse::Ok)), "{removed:?}");
    assert!(!Path::new(&path).exists(), "removed once it was made");
}

/// A hub that finds a workspace an earlier one was archiving asks whether its worktree is there, and hears how the
/// removal the worker is still carrying out ended, rather than that it's there while it goes.
#[tokio::test]
async fn whether_a_worktree_is_there_is_answered_once_its_removal_has_ended() {
    let served = served().await;
    let state = &served.worker.state;
    let own = served.own.clone();

    // Joined, so the removal is polled first and has the worktree to itself before the question is asked.
    let (removed, there) =
        tokio::join!(remove(state, served.removing(&own, false)), there(state, WorktreeThere { path: own.clone() }));

    assert!(matches!(removed, Ok(WorkerResponse::Ok)), "{removed:?}");
    assert_eq!(there.unwrap(), WorkerResponse::WorktreeThere(WorktreeThereReply { there: false }));
}

/// Putting a worktree back takes its turn too, so a hub that finds a workspace an earlier one was restoring hears it's
/// there, rather than that it isn't while git is still making it.
#[tokio::test]
async fn whether_a_worktree_is_there_is_answered_once_it_s_been_put_back() {
    let served = served().await;
    let state = &served.worker.state;
    let own = served.own.clone();
    remove(state, served.removing(&own, false)).await.unwrap();
    let restoring = RestoreWorkspace {
        source: served.source.clone(),
        path: own.clone(),
        branch: "alder/start".into(),
        archived: None,
    };

    // Joined, so the restore is polled first and has the worktree to itself before the question is asked.
    let (restored, there) = tokio::join!(restore(state, restoring), there(state, WorktreeThere { path: own }));

    assert!(matches!(restored, Ok(WorkerResponse::Ok)), "{restored:?}");
    assert_eq!(there.unwrap(), WorkerResponse::WorktreeThere(WorktreeThereReply { there: true }));
}
