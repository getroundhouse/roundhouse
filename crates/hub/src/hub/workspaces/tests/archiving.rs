//! Archiving a workspace, on a hub whose workspaces have stand-ins for their workers: what it refuses before anything
//! changes.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::WorkspaceStatus;
use protocol::prompt::Prompt;
use protocol::worker::WorkerRequest;

use crate::hub::stand_in_workers::{TIMEOUT, a_chat, a_turn, answering_first, hub_with_stand_in_workers};

/// A worktree whose status can't be read, such as one whose folder was deleted by hand or whose index is damaged,
/// can't say that archiving it would lose nothing, so archiving is refused there, with `NeedsForce`, before setup is
/// stopped, the archive script runs, the agents are stopped or anything is removed. Forced, it's archived.
#[tokio::test]
async fn a_worktree_whose_status_cant_be_read_is_archived_only_when_forced() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    waiting::task::until(TIMEOUT, "the first setup to finish", || hub.setup_gates.lock().is_empty()).await;
    a_turn(&hub, &asked, &chat.id, Prompt::new("go"), None).await;
    answering_first(&hub, &workspace.id, &asked, |request| {
        let WorkerRequest::GitStatus(_) = request else { return None };
        Some(Err(ApiError::new(ErrorCode::Worker, "not a git repository")))
    });
    let before = asked.requests(|_| Some(())).len();

    let refused = hub.workspace_archive(workspace.id.clone(), false).await;

    let refused = refused.expect_err("archived unforced, though its status couldn't be read");
    assert_eq!(refused.code, ErrorCode::NeedsForce, "so whoever asked is offered a forced archive: {refused:?}");
    assert!(refused.message.contains("not a git repository"), "it says what couldn't be read: {}", refused.message);
    let archiving: Vec<_> = asked.requests(|request| Some(request.clone())).split_off(before);
    assert!(matches!(archiving.first(), Some(WorkerRequest::GitStatus(_))), "its status was read: {archiving:?}");
    let changing: Vec<_> = archiving.iter().filter(|request| !matches!(request, WorkerRequest::GitStatus(_))).collect();
    assert!(changing.is_empty(), "refusing asked its worker for more than statuses: {changing:?}");
    assert!(hub.live_session(&chat.id).await.unwrap().agent_running(), "the chat's agent was stopped");
    assert_eq!(hub.store.get_workspace(&workspace.id).unwrap().status, WorkspaceStatus::Ready);

    let forced = hub.workspace_archive(workspace.id.clone(), true).await.unwrap();

    assert_eq!(forced.status, WorkspaceStatus::Archived);
}
