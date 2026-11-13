//! Removing a workspace's other worktrees: one for an agent that asks, and all of them as the workspace is archived.
//! Each keeps its branch and its record.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ChildWorktree, Repo, WorkspaceId};
use protocol::worker::{RemoveWorkspace, WorktreeRemoval, WorktreeRemoved, WouldLose};
use tracing::warn;

use super::git_status;
use crate::hub::Hub;
use crate::hub::ready::ReadyWorkspace;
use crate::worker_link::WorkerLink;

impl Hub {
    /// Removes one of the workspace's other worktrees for an agent, as a
    /// [`RemoveWorktree`](protocol::worker::RemoveWorktree) asks. One removed already answers that it's gone, rather
    /// than failing.
    pub(in crate::hub) async fn remove_child(
        &self,
        workspace_id: &WorkspaceId,
        link: &WorkerLink,
        path: String,
        force: bool,
    ) -> Result<WorktreeRemoval, ApiError> {
        let _removing = self.child_gate(workspace_id).write_owned().await;
        let ready = self.ready_workspace(workspace_id).await?;
        let (id, at) = (ready.id().clone(), path.clone());
        let Some(child) = self.blocking(move |store| store.child_at(&id, &at)).await? else {
            return self.not_a_child(&ready, path).await;
        };
        let last = git_status(link, &child.path).await;
        if !force && let Some(status) = last.clone().filter(|status| status.loss_if_removed().is_some()) {
            return Ok(WorktreeRemoval::WouldLose(WouldLose { status }));
        }
        let repo = self.repo_of(ready.workspace()).await?;
        let worktree = self.remove_one(&repo, link, ready.path(), &child, force).await?;
        Ok(WorktreeRemoval::Removed(WorktreeRemoved { worktree, last }))
    }

    /// The answer for a path none of the workspace's other worktrees that's there has.
    async fn not_a_child(&self, ready: &ReadyWorkspace, path: String) -> Result<WorktreeRemoval, ApiError> {
        if ready.path() == path {
            let own = "that's this workspace's own worktree, which goes when the workspace is archived";
            return Err(ApiError::new(ErrorCode::Conflict, own));
        }
        let id = ready.id().clone();
        let all = self.blocking(move |store| store.every_child_of(&id)).await?;
        match all.into_iter().rev().find(|child| child.path == path) {
            Some(worktree) => Ok(WorktreeRemoval::Removed(WorktreeRemoved { worktree, last: None })),
            None => Err(ApiError::new(ErrorCode::NotFound, format!("{path} isn't one of this workspace's worktrees"))),
        }
    }

    /// Removes each of the workspace's other worktrees as it's archived, with its gate held to write. Each checks
    /// again unless forced, since an agent may have written in one before it stopped. One that isn't removed, for
    /// that or because it can't be, such as a directory that's no longer a worktree, refuses with `NeedsForce`,
    /// saying which it is, since forcing goes on past it: it's left and logged, as a failing archive script is, so it
    /// can't keep the workspace from being archived.
    pub(in crate::hub) async fn remove_children(
        &self,
        repo: &Repo,
        link: &WorkerLink,
        own: &str,
        children: &[ChildWorktree],
        force: bool,
    ) -> Result<(), ApiError> {
        for child in children {
            match self.remove_one(repo, link, own, child, force).await {
                Ok(_) => {}
                Err(e) if force => warn!(path = child.path, "couldn't remove another worktree: {}", e.message),
                Err(e) => {
                    let left = format!("it couldn't be removed ({}). Archiving with force leaves it there", e.message);
                    let why = if e.code == ErrorCode::NeedsForce { &e.message } else { &left };
                    return Err(ApiError::new(ErrorCode::NeedsForce, within(child, why)));
                }
            }
        }
        Ok(())
    }

    /// Has the worker remove `child`, beside the workspace's own worktree at `own`, and records it removed.
    async fn remove_one(
        &self,
        repo: &Repo,
        link: &WorkerLink,
        own: &str,
        child: &ChildWorktree,
        force: bool,
    ) -> Result<ChildWorktree, ApiError> {
        let removing =
            RemoveWorkspace { repo_path: repo.path.clone(), path: child.path.clone(), force, own: Some(own.into()) };
        link.call(removing).await?;
        let id = child.id.clone();
        self.blocking(move |store| store.child_removed(&id)).await
    }
}

/// `said`, of the worktree `child`, in words for a person, who has to be told which worktree it's about.
fn within(child: &ChildWorktree, said: &str) -> String {
    format!("In {}: {said}", child.path)
}

/// What archiving the workspace would lose in `children`, its other worktrees, in words for a person, naming each.
pub(in crate::hub) async fn would_lose(link: &WorkerLink, children: &[ChildWorktree]) -> Vec<String> {
    let mut losses = Vec::new();
    for child in children {
        if let Some(loss) = git_status(link, &child.path).await.and_then(|status| status.loss_if_removed()) {
            losses.push(within(child, &loss));
        }
    }
    losses
}
