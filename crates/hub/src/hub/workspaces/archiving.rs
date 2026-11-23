//! Archiving and restoring workspaces. The record says a workspace is being archived or restored from the moment it's
//! asked, so every client shows it, and only one request can do it at once. It goes back as it was if that's refused
//! or fails. A hub that stops part way leaves it for the next, which asks the workspace's worker whether the worktree
//! is there and records what that says.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{BranchName, Workspace, WorkspaceId};
use protocol::worker::{ArchiveScript, CancelSetup, GitStatus, RemoveWorkspace, RestoreWorkspace};
use protocol::worker::{WorktreeThere, WorktreeThereReply};
use tokio::sync::oneshot;
use tracing::{error, info, warn};

use super::ephemeral::uses_ephemeral_clone;

use crate::error::internal;
use crate::hub::Hub;
use crate::hub::children::{archive_would_lose, would_lose};
use crate::hub::ready::ReadyWorkspace;
use crate::hub::repo_source::workspace_env_for;
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
        // Its worker stops watching the worktree, which isn't ready any more, before the worktree goes.
        self.changes_watching(&id).await;
        // Until it's archived or put back, and taken once its other worktrees being made are made, so every one of them
        // is removed with it, and none is made after. Its record says it's being archived by now, so none starts.
        let _removing = self.child_gate(&id).write_owned().await;
        let branch = match self.remove_worktree(&workspace, path, force).await {
            Ok(branch) => branch,
            Err(e) => return Err(self.put_back(&id, Move::Archive, e).await),
        };
        // Once the worktree (and any ephemeral instance) is gone, never put the workspace back to Ready if recording
        // fails: leaving it Archiving lets startup settling finish it without tidying its saved bundle.
        self.record_archived(&id, branch).await
    }

    /// Records that the workspace is archived, on `branch` if its worktree said which it was on, as
    /// [`Hub::end_move`] does, and forgets its lines and the files it changed, as [`Hub::changes_forgotten`] says.
    pub(super) async fn record_archived(
        self: &Arc<Self>,
        id: &WorkspaceId,
        branch: Option<BranchName>,
    ) -> Result<Workspace, ApiError> {
        if let Some(branch) = branch {
            let archived = id.clone();
            self.blocking(move |store| store.set_workspace_branch(&archived, &branch)).await?;
        }
        let workspace = self.end_move(id, Move::Archive, Ended::Done, async |archived| Ok(archived)).await?;
        self.changes_forgotten(id).await;
        Ok(workspace)
    }

    /// Everything archiving does but record it: refuses if work would be lost, in the workspace's own worktree or any
    /// other made for it, unless forced, runs the archive script, stops the agents and removes the worktrees. Returns
    /// the branch the workspace's own was on, which the agent may have renamed. With the gate of its other worktrees
    /// held to write.
    async fn remove_worktree(
        self: &Arc<Self>,
        workspace: &Workspace,
        path: String,
        force: bool,
    ) -> Result<Option<BranchName>, ApiError> {
        let id = &workspace.id;
        let placement = self.workspace_placement(id).await?;
        let ephemeral = uses_ephemeral_clone(&placement);
        let provider = self.provider(&workspace.provider)?;
        if ephemeral {
            // Clear any stale archive left by a failed cleanup on an earlier restore before capturing a new one.
            self.remove_archived_git(id).await;
        }
        let link = self.ensure_worker(id).await?;
        let owned = id.clone();
        let children = self.blocking(move |store| store.children_of(&owned)).await?;

        // Before anything changes, so refusing leaves the workspace as it was. A status that can't be read refuses too,
        // as `archive_would_lose` says, and forcing goes on without it.
        let status = link.call(GitStatus { path: path.clone() }).await;
        if !force {
            // Every worktree's, so forcing past one's doesn't lose another's untold.
            let own = archive_would_lose(&status);
            let mut losses: Vec<String> = own.into_iter().chain(would_lose(&link, &children).await).collect();
            if ephemeral
                && !losses.is_empty()
                && let Some(oversized) = Self::probe_oversized_archive(&link, &placement.source).await
            {
                losses.push(oversized);
            }
            if !losses.is_empty() {
                return Err(ApiError::new(ErrorCode::NeedsForce, losses.join("\n")));
            }
        }

        // Interrupt setup first, so the script isn't running in a worktree that's about to go.
        let _ = link.call(CancelSetup).await;
        self.call_off_waits_of(id).await?;
        let env = workspace_env_for(workspace, &placement);
        let script = ArchiveScript { source: placement.source.clone(), path: path.clone(), env };
        match link.call(script).await {
            Ok(()) => {}
            Err(e) if !force => return Err(e),
            Err(e) => warn!(workspace = workspace.name, "the archive script failed: {}", e.message),
        }

        self.stop_workspace_agents(id).await?;
        // Again, now that none can start one, since an agent still running as the script ran could have, and marking the
        // workspace archived would end that one with no client told.
        self.call_off_waits_of(id).await?;
        // Its other worktrees first, so one that refuses leaves the workspace's own, and so does a hub that stops part
        // way, which the next then finds ready. Each checks again unless forced, because an agent may have written
        // files before it stopped.
        self.remove_children(&placement.source, &link, &path, &children, force).await?;
        if ephemeral {
            // Capture and persist durably BEFORE removing the worktree, so a size refusal, disk error or older worker
            // leaves the worktree untouched.
            self.capture_and_save_archive(workspace, &link, &placement.source, force).await?;
        }
        if let Err(e) = link.call(RemoveWorkspace { source: placement.source, path, force, own: None }).await {
            if ephemeral {
                self.remove_archived_git(id).await;
            }
            return Err(e);
        }
        if ephemeral {
            self.destroy_ephemeral_instance(workspace, &link, provider.as_ref()).await;
        } else {
            self.close_worker(workspace, &link, provider.as_ref()).await;
        }
        // Agents are expected to rename their branch to match the work. Restoring needs its real name.
        Ok(match status {
            Ok(status) => status.branch,
            Err(_) => None,
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
        let link = match self.make_worktree_again(&workspace).await {
            Ok(made) => made,
            Err(e) => return Err(self.put_back(&id, Move::Restore, e).await),
        };
        match self.record_restored(&id, &link).await {
            Ok(restored) => Ok(restored),
            Err(e) => Err(self.put_back(&id, Move::Restore, e).await),
        }
    }

    /// Records that the workspace is ready again, now its worktree is back, as [`Hub::end_move`] does, and starts its
    /// setup. Returns it as recorded.
    pub(super) async fn record_restored(
        self: &Arc<Self>,
        id: &WorkspaceId,
        link: &Arc<WorkerLink>,
    ) -> Result<Workspace, ApiError> {
        let restored = self
            .end_move(id, Move::Restore, Ended::Done, async |restored| {
                // Once `Ready` is durable in SQLite, any saved archive bundle is no longer needed.
                self.remove_archived_git(id).await;
                let restored = ReadyWorkspace::new(restored)?;
                // The worktree has only the branch it's restored on now, which its record says before it's announced.
                self.watch_branches_from_now(&restored, link).await;
                Ok(self.ready_workspace(restored.id()).await?.into_workspace())
            })
            .await?;
        // A restored worktree has lost what setup made, since none of it is committed.
        self.start_setup(ReadyWorkspace::new(restored)?).await
    }

    /// Has the workspace's worker make its worktree again from its branch. Returns the worker.
    async fn make_worktree_again(self: &Arc<Self>, workspace: &Workspace) -> Result<Arc<WorkerLink>, ApiError> {
        let path = workspace.path.clone().ok_or_else(|| internal("an archived workspace has no path"))?;
        let placement = self.workspace_placement(&workspace.id).await?;
        if uses_ephemeral_clone(&placement) {
            return self.restore_ephemeral(workspace, path, placement).await;
        }
        let link = self.ensure_worker(&workspace.id).await?;
        let request =
            RestoreWorkspace { source: placement.source, path, branch: workspace.branch.clone(), archived: None };
        link.call(request).await?;
        Ok(link)
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

    /// Puts the workspace back as it was before it was being archived or restored, as [`Hub::end_move`] does.
    pub(super) async fn end_move_put_back(
        self: &Arc<Self>,
        id: &WorkspaceId,
        moving: Move,
    ) -> Result<Workspace, ApiError> {
        self.end_move(id, moving, Ended::PutBack, async |put_back| Ok(put_back)).await
    }

    /// Puts the workspace back as it was before it was being archived or restored, which was refused or failed for
    /// `why`, as [`Hub::end_move_put_back`] does. Returns `why`.
    async fn put_back(self: &Arc<Self>, id: &WorkspaceId, moving: Move, why: ApiError) -> ApiError {
        if let Err(e) = self.end_move_put_back(id, moving).await {
            error!(%id, ?moving, "putting back a workspace: {}", e.message);
        }
        why
    }

    /// Ends the workspace's move as `ended`, the one place every move ends: records it, gives the workspace as recorded
    /// to `before_announcing`, announces the workspace that gives back, and tells the workspace's worker again whether
    /// to watch its worktree. Its worker watches only while the workspace is ready, and only a move changes that once
    /// the workspace is made, so a move that ended anywhere else could leave a workspace a client shows unwatched.
    /// Returns the workspace as announced.
    async fn end_move(
        self: &Arc<Self>,
        id: &WorkspaceId,
        moving: Move,
        ended: Ended,
        before_announcing: impl AsyncFnOnce(Workspace) -> Result<Workspace, ApiError>,
    ) -> Result<Workspace, ApiError> {
        let owned = id.clone();
        let recorded = self.blocking(move |store| store.end_move(&owned, moving, ended)).await?;
        let announced = before_announcing(recorded).await;
        if let Ok(workspace) = &announced {
            self.notify(HubNotice::Workspace(workspace.clone()));
        }
        // Announced or not, it's recorded, and awaited, so whoever asked for the move is answered once it's told.
        self.changes_watching(id).await;
        announced
    }

    /// Settles each workspace an earlier hub left part way through being archived or restored, once this one is
    /// serving, so a worker that's slow to answer keeps no client waiting. Until then its record says it's being
    /// archived or restored, as it did, and nothing else can be asked of it. Once every move has settled, tidies any
    /// stale archive bundles.
    pub(crate) async fn settle_moves(self: Arc<Self>, left: Vec<(WorkspaceId, Move)>) {
        for (id, moving) in left {
            match self.settle_move(&id, moving).await {
                Ok(workspace) => {
                    let status = workspace.status;
                    info!(workspace = workspace.name, ?status, "settled a workspace the last run was moving");
                }
                Err(e) => {
                    warn!(%id, ?moving, "couldn't tell how far the last run got, so putting it back: {}", e.message);
                    self.put_back(&id, moving, e).await;
                }
            }
            self.worker_once_settled(&id, moving).await;
        }
        self.tidy_bundles().await;
    }

    /// What becomes of the worker the workspace was settled with, which the hub kept as it started, or started to
    /// ask: an archived workspace has none, and a ready one's is as any the hub carries on with, its branches watched
    /// from now, and replaced once it's idle if it's another build. A restore ends ready only by being recorded as done,
    /// which has the worker watch the branches as it's recorded, as one that wasn't cut short does, so it isn't asked
    /// again here.
    async fn worker_once_settled(self: &Arc<Self>, id: &WorkspaceId, moving: Move) {
        let Some(link) = self.existing_worker(id) else { return };
        let workspace = match self.workspace(id).await {
            Ok(workspace) => workspace,
            Err(e) => return warn!(%id, "reading a workspace that was settled: {}", e.message),
        };
        match ReadyWorkspace::new(workspace.clone()) {
            Ok(ready) => {
                match moving {
                    Move::Archive => self.watch_branches(&ready, &link).await,
                    Move::Restore => {}
                }
                // One with a session carried on is being replaced already.
                let carried_on = self.sessions.lock().live.values().any(|session| session.workspace_id == *id);
                if !carried_on && link.is_another_build(self.build) {
                    self.replace_once_idle(id.clone(), link);
                }
            }
            Err(_) => match self.provider(&workspace.provider) {
                Ok(provider) => self.close_worker(&workspace, &link, provider.as_ref()).await,
                Err(e) => warn!(%id, "stopping the worker of a workspace settled as archived: {}", e.message),
            },
        }
    }

    /// Records what the workspace's worker finds: a worktree that's there is a ready workspace's, and one that isn't
    /// is an archived one's, whichever the earlier hub was doing. Fails if the worker can't say, such as one from a
    /// build that doesn't know the question.
    async fn settle_move(self: &Arc<Self>, id: &WorkspaceId, moving: Move) -> Result<Workspace, ApiError> {
        let workspace = self.workspace(id).await?;
        let path = workspace.path.clone().ok_or_else(|| internal("a workspace being moved has no path"))?;
        let placement = self.workspace_placement(id).await?;
        if uses_ephemeral_clone(&placement) {
            return self.settle_ephemeral_move(&workspace, path, placement, moving).await;
        }
        let link = self.ensure_worker(id).await?;
        let WorktreeThereReply { there } = link.call(WorktreeThere { path }).await?;
        match (moving, there) {
            // It got no further than the worktree, so the workspace is as it was before.
            (Move::Archive, true) | (Move::Restore, false) => self.end_move_put_back(id, moving).await,
            (Move::Archive, false) => {
                // The worktree can't say which branch it was on any more. What its worker last read of it can.
                self.record_archived(id, checked_out_branch(&workspace)).await
            }
            (Move::Restore, true) => self.record_restored(id, &link).await,
        }
    }
}

pub(super) fn checked_out_branch(workspace: &Workspace) -> Option<BranchName> {
    workspace.branches.as_ref().and_then(|branches| branches.checked_out()).map(|branch| branch.name.clone())
}
