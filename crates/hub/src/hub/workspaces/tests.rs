//! Settling a workspace an earlier hub left part way through being archived or restored.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{WorkspaceId, WorkspaceStatus};
use protocol::worker::{WorkerRequest, WorkerResponse, WorktreeThereReply};

use crate::hub::tests::{hub_in, serving};
use crate::store::{Ended, Move};
use crate::worker_link::WorkerLink;

/// The worker that was putting the worktree back says whether it's there once it has finished, so the hub records what
/// it says, even with the worktree's folder there, which git makes before it has finished with it.
#[tokio::test]
async fn a_restore_cut_short_is_settled_by_what_its_worker_says_rather_than_by_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let id = WorkspaceId::from("w");
    let worktree = dir.path().join("ws/alder");
    std::fs::create_dir_all(&worktree).unwrap();
    hub.store.workspace_ready(&id, worktree.to_str().unwrap(), &"alder".into(), "main", None).unwrap();
    hub.store.start_move(&id, Move::Archive).unwrap();
    hub.store.end_move(&id, Move::Archive, Ended::Done).unwrap();
    hub.store.start_move(&id, Move::Restore).unwrap();
    let link = WorkerLink::answering(|request| {
        Some(if matches!(request, WorkerRequest::WorktreeThere(_)) {
            Ok(WorkerResponse::WorktreeThere(WorktreeThereReply { there: false }))
        } else {
            Err(ApiError::new(ErrorCode::Worker, "a stand-in does nothing else"))
        })
    });
    serving(&hub, &link);

    hub.clone().settle_moves(vec![(id.clone(), Move::Restore)]).await;

    assert_eq!(hub.store.get_workspace(&id).unwrap().status, WorkspaceStatus::Archived);
}
