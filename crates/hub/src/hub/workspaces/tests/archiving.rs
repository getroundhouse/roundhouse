//! Archiving a workspace, on a hub whose workspaces have stand-ins for their workers: what it refuses before anything
//! changes.

use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures;
use protocol::model::{ChildWorktree, WorkspaceStatus};
use protocol::prompt::Prompt;
use protocol::worker::{GitStatus, WorkerRequest};

use crate::hub::stand_in_workers::{TIMEOUT, a_chat, a_turn, answering_first, hub_with_stand_in_workers};

/// Where the workspace's other worktree is.
const OTHER: &str = "/ws/.children/try";

/// A worktree whose status can't be read, such as one whose folder was deleted by hand or whose index is damaged,
/// can't say that archiving it would lose nothing, so archiving is refused there, with `NeedsForce`, before setup is
/// stopped, the archive script runs, the agents are stopped or anything is removed. That holds for the workspace's own
/// worktree and for another made for it, which the refusal names. Forced, it's archived.
#[tokio::test]
async fn a_worktree_whose_status_cant_be_read_is_archived_only_when_forced() {
    for (case, its_own) in [("its own worktree", true), ("another of its worktrees", false)] {
        let dir = tempfile::tempdir().unwrap();
        let (hub, asked) = hub_with_stand_in_workers(dir.path());
        let (workspace, chat) = a_chat(&hub).await;
        waiting::task::until(TIMEOUT, "the first setup to finish", || hub.setup_gates.lock().is_empty()).await;
        a_turn(&hub, &asked, &chat.id, Prompt::new("go"), None).await;
        let other = ChildWorktree {
            workspace_id: workspace.id.clone(),
            session_id: chat.id.clone(),
            path: OTHER.into(),
            ..fixtures::child_worktree("try")
        };
        hub.store.insert_child(&other, &"k".into()).unwrap();
        hub.store.child_ready(&other.id, None).unwrap();
        let unreadable = if its_own { workspace.path.clone().unwrap() } else { OTHER.to_string() };
        answering_first(&hub, &workspace.id, &asked, move |request| {
            let WorkerRequest::GitStatus(GitStatus { path }) = request else { return None };
            (*path == unreadable).then(|| Err(ApiError::new(ErrorCode::Worker, "not a git repository")))
        });
        let before = asked.requests(|_| Some(())).len();

        let refused = hub.workspace_archive(workspace.id.clone(), false).await;

        let refused = refused.expect_err(&format!("{case}: archived unforced, though a status couldn't be read"));
        assert_eq!(refused.code, ErrorCode::NeedsForce, "{case}: so whoever asked is offered to force: {refused:?}");
        let said = &refused.message;
        assert!(said.contains("not a git repository"), "{case}: it says what couldn't be read: {said}");
        let names_other = said.starts_with(&format!("In {OTHER}: "));
        assert_eq!(names_other, !its_own, "{case}: it names the worktree only when it isn't the workspace's: {said}");
        let archiving: Vec<_> = asked.requests(|request| Some(request.clone())).split_off(before);
        let read = |request: &&WorkerRequest| matches!(request, WorkerRequest::GitStatus(_));
        assert_eq!(archiving.iter().filter(read).count(), 2, "{case}: both statuses were read: {archiving:?}");
        let changing: Vec<_> = archiving.iter().filter(|request| !read(request)).collect();
        assert!(changing.is_empty(), "{case}: refusing asked its worker for more than statuses: {changing:?}");
        assert!(hub.live_session(&chat.id).await.unwrap().agent_running(), "{case}: the chat's agent was stopped");
        assert_eq!(hub.store.get_workspace(&workspace.id).unwrap().status, WorkspaceStatus::Ready, "{case}");

        let forced = hub.workspace_archive(workspace.id.clone(), true).await.unwrap();

        assert_eq!(forced.status, WorkspaceStatus::Archived, "{case}");
    }
}
