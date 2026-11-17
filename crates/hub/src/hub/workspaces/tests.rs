//! Making a workspace, with a stand-in for its worker, and settling one an earlier hub left part way through being
//! archived or restored.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AgentKind, ClientRequestId, SessionSettingsChange, WorkspaceId, WorkspaceStatus};
use protocol::worker::{PrepareWorkspace, RepoSource, WorkerRequest, WorkerResponse, WorktreeThereReply};

use crate::hub::stand_in_workers::hub_with_stand_in_workers;
use crate::hub::tests::{hub_in, serving};
use crate::store::{Ended, Move};
use crate::worker_link::WorkerLink;

/// The worktree a worker was asked to make.
fn worktree_asked_for(request: &WorkerRequest) -> Option<PrepareWorkspace> {
    let WorkerRequest::PrepareWorkspace(prepare) = request else { return None };
    Some(prepare.clone())
}

/// Its worker is asked for a worktree from the repository's checkout, on a branch in a folder of the workspace's name,
/// and where the worker says it made it is where the workspace is.
#[tokio::test]
async fn a_workspace_is_ready_where_its_worker_made_the_worktree_it_was_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());

    let made = hub.workspace_create("r".into(), Some("birch".into()), Some("origin/next".into()), None).await.unwrap();

    let asked_for = PrepareWorkspace {
        repo_name: "repo".to_string(),
        source: RepoSource::Checkout { path: "/repo".to_string() },
        workspace_name: "birch".to_string(),
        branch: "birch/start".into(),
        base_ref: Some("origin/next".to_string()),
    };
    assert_eq!(asked.requests(worktree_asked_for), [asked_for]);
    assert_eq!(
        (made.status, made.path.as_deref(), made.branch.as_str(), made.base_ref.as_deref()),
        (WorkspaceStatus::Ready, Some("/ws/birch"), "birch/start", Some("origin/next"))
    );
    assert_eq!(hub.workspace(&made.id).await.unwrap().path.as_deref(), Some("/ws/birch"), "as its record says");
}

/// So a client that loses the reply can send it again, whatever has happened meanwhile, and get what it made rather
/// than a refusal, or a second one. Here the workspace has been archived since, where a new chat is refused.
#[tokio::test]
async fn a_request_sent_again_gets_what_it_made_however_things_have_changed_since() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let a_workspace = || hub.workspace_create("r".into(), None, None, Some("a workspace".into()));
    let workspace = a_workspace().await.unwrap();
    let a_chat = |request: &str| {
        let (unchanged, request) = (SessionSettingsChange::default(), ClientRequestId::from(request));
        hub.session_create(workspace.id.clone(), AgentKind::Claude, unchanged, Some(request))
    };
    let chat = a_chat("a chat").await.unwrap();
    hub.workspace_archive(workspace.id.clone(), true).await.unwrap();

    let (workspace_again, chat_again) = (a_workspace().await.unwrap(), a_chat("a chat").await.unwrap());

    assert_eq!((&workspace_again.id, workspace_again.status), (&workspace.id, WorkspaceStatus::Archived));
    assert_eq!(chat_again.id, chat.id);
    assert_eq!(asked.requests(worktree_asked_for).len(), 1, "its worker wasn't asked for a second worktree");
    let another = a_chat("another chat").await.map_err(|refused| refused.code);
    assert_eq!(another.err(), Some(ErrorCode::Conflict), "a new one is checked, and refused");
}

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
