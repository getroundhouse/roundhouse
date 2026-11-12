//! Archiving and restoring workspaces. The record says a workspace is being archived or restored from the moment it's
//! asked, so every client shows it, and only one request can do it at once. It goes back as it was if that's refused
//! or fails, and a hub that stops part way leaves it for the next to put back as it starts.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{BranchName, Repo, Workspace, WorkspaceId};
use protocol::worker::{WorkerRequest, WorkerResponse};
use tokio::sync::oneshot;
use tracing::{error, warn};

use super::workspace_env;
use crate::error::internal;
use crate::hub::ready::ReadyWorkspace;
use crate::hub::{Hub, unexpected};
use crate::session::Command;
use crate::store::{Ended, Move};
use crate::worker_link::WorkerLink;

impl Hub {
    pub(crate) async fn workspace_archive(
        self: &Arc<Self>,
        id: WorkspaceId,
        force: bool,
    ) -> Result<Workspace, ApiError> {
        // Refused as everything that needs a ready workspace refuses one that isn't.
        let (_, path) = self.ready_workspace(&id).await?.into_parts();
        let workspace = self.start_move(&id, Move::Archive).await?;
        let branch = match self.remove_worktree(&workspace, path, force).await {
            Ok(branch) => branch,
            Err(e) => return Err(self.put_back(&id, Move::Archive, e).await),
        };
        let archived = id.clone();
        let recorded = self
            .blocking(move |store| {
                if let Some(branch) = branch {
                    store.set_workspace_branch(&archived, &branch)?;
                }
                store.end_move(&archived, Move::Archive, Ended::Done)
            })
            .await;
        match recorded {
            Ok(workspace) => {
                self.notify(HubNotice::Workspace(workspace.clone()));
                Ok(workspace)
            }
            Err(e) => Err(self.put_back(&id, Move::Archive, e).await),
        }
    }

    /// Everything archiving does but record it: refuses if work would be lost, unless forced, runs the archive script,
    /// stops the agents and removes the worktree. Returns the branch the worktree was on, which the agent may have
    /// renamed.
    async fn remove_worktree(
        self: &Arc<Self>,
        workspace: &Workspace,
        path: String,
        force: bool,
    ) -> Result<Option<BranchName>, ApiError> {
        let id = &workspace.id;
        let repo = {
            let repo_id = workspace.repo_id.clone();
            self.blocking(move |store| store.get_repo(&repo_id)).await?
        };
        let link = self.ensure_worker(id).await?;

        // Before anything changes, so refusing leaves the workspace as it was.
        let status = link.call(WorkerRequest::GitStatus { path: path.clone() }).await;
        if !force
            && let Ok(WorkerResponse::GitStatus(status)) = &status
            && let Some(loss) = status.loss_if_removed()
        {
            return Err(ApiError::new(ErrorCode::NeedsForce, loss));
        }

        // Interrupt setup first, so the script isn't running in a worktree that's about to go.
        let _ = link.call(WorkerRequest::CancelSetup).await;
        self.call_off_waits_of(id).await?;
        let script = WorkerRequest::ArchiveScript {
            repo_path: repo.path.clone(),
            path: path.clone(),
            env: workspace_env(workspace, &repo),
        };
        match link.call(script).await {
            Ok(_) => {}
            Err(e) if !force => return Err(e),
            Err(e) => warn!(workspace = workspace.name, "the archive script failed: {}", e.message),
        }

        self.stop_workspace_agents(id).await?;
        // Again, now that none can start one, since an agent still running as the script ran could have, and marking the
        // workspace archived would end that one with no client told.
        self.call_off_waits_of(id).await?;
        // Checks again unless forced, because the agent may have written files before it stopped.
        let reply = link.call(WorkerRequest::RemoveWorkspace { repo_path: repo.path, path, force }).await?;
        if !matches!(reply, WorkerResponse::Ok) {
            return Err(unexpected(&reply));
        }
        let provider = self.provider(&workspace.provider)?;
        self.close_worker(workspace, &link, provider.as_ref()).await;
        // Agents are expected to rename their branch to match the work. Restoring needs its real name.
        Ok(match status {
            Ok(WorkerResponse::GitStatus(status)) => status.branch,
            _ => None,
        })
    }

    async fn stop_workspace_agents(self: &Arc<Self>, workspace_id: &WorkspaceId) -> Result<(), ApiError> {
        let sessions: Vec<_> = {
            let owned = workspace_id.clone();
            self.blocking(move |store| store.list_sessions(&owned)).await?
        };
        for session in sessions {
            let live = self.live_session(&session.id).await?;
            if !live.agent_running() {
                // Its waits were called off as archiving started. Archiving goes on whether or not they come off its list.
                if let Err(e) = live.unlist_waits().await {
                    warn!(session = %session.id, "couldn't take an archived chat's waits off its list: {e}");
                }
                continue;
            }
            let (reply, done) = oneshot::channel();
            live.command(Command::Stop { reply }).await?;
            done.await.map_err(|_| internal("the session has stopped"))??;
        }
        Ok(())
    }

    pub(crate) async fn workspace_restore(self: &Arc<Self>, id: WorkspaceId) -> Result<Workspace, ApiError> {
        let workspace = self.start_move(&id, Move::Restore).await?;
        let (repo, link) = match self.make_worktree_again(&workspace).await {
            Ok(made) => made,
            Err(e) => return Err(self.put_back(&id, Move::Restore, e).await),
        };
        let ready = id.clone();
        let restored = match self.blocking(move |store| store.end_move(&ready, Move::Restore, Ended::Done)).await {
            Ok(restored) => ReadyWorkspace::new(restored)?,
            Err(e) => return Err(self.put_back(&id, Move::Restore, e).await),
        };
        // The worktree has only the branch it's restored on now, which its record says before it's announced.
        self.watch_branches_from_now(&restored, &link).await;
        let restored = self.ready_workspace(restored.id()).await?;
        self.notify(HubNotice::Workspace(restored.workspace().clone()));
        // A restored worktree has lost what setup made, since none of it is committed.
        self.start_setup(repo, restored).await
    }

    /// Has the workspace's worker make its worktree again from its branch. Returns the workspace's repository, and the
    /// worker.
    async fn make_worktree_again(self: &Arc<Self>, workspace: &Workspace) -> Result<(Repo, Arc<WorkerLink>), ApiError> {
        let repo = {
            let repo_id = workspace.repo_id.clone();
            self.blocking(move |store| store.get_repo(&repo_id)).await?
        };
        let path = workspace.path.clone().ok_or_else(|| internal("an archived workspace has no path"))?;
        let link = self.ensure_worker(&workspace.id).await?;
        let request =
            WorkerRequest::RestoreWorkspace { repo_path: repo.path.clone(), path, branch: workspace.branch.clone() };
        let reply = link.call(request).await?;
        if !matches!(reply, WorkerResponse::Ok) {
            return Err(unexpected(&reply));
        }
        Ok((repo, link))
    }

    /// Records that the workspace is being archived or restored, and announces it. Refuses one that isn't where that
    /// starts from, such as one that another request is archiving already.
    async fn start_move(&self, id: &WorkspaceId, moving: Move) -> Result<Workspace, ApiError> {
        let owned = id.clone();
        if let Some(started) = self.blocking(move |store| store.start_move(&owned, moving)).await? {
            self.notify(HubNotice::Workspace(started.clone()));
            return Ok(started);
        }
        // Another request moved it first.
        let workspace = self.workspace(id).await?;
        Err(match moving {
            Move::Archive => {
                ApiError::new(ErrorCode::Conflict, format!("workspace {} is {:?}", workspace.name, workspace.status))
            }
            Move::Restore => ApiError::new(ErrorCode::Conflict, format!("workspace {} isn't archived", workspace.name)),
        })
    }

    /// Puts the workspace back as it was before it was being archived or restored, which was refused or failed for
    /// `why`, and announces it. Returns `why`.
    async fn put_back(&self, id: &WorkspaceId, moving: Move, why: ApiError) -> ApiError {
        let owned = id.clone();
        match self.blocking(move |store| store.end_move(&owned, moving, Ended::PutBack)).await {
            Ok(workspace) => self.notify(HubNotice::Workspace(workspace)),
            Err(e) => error!(%id, ?moving, "putting back a workspace: {}", e.message),
        }
        why
    }
}
