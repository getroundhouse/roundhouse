//! Settling a workspace an earlier hub left part way through being archived or restored, by what its worker finds.

use std::sync::Arc;

use protocol::client::ApiError;
use protocol::model::{Workspace, WorkspaceId};
use protocol::worker::{WorktreeThere, WorktreeThereReply};
use tracing::{info, warn};

use super::ephemeral::uses_ephemeral_clone;

use crate::error::internal;
use crate::hub::Hub;
use crate::hub::ready::ReadyWorkspace;
use crate::store::Move;

impl Hub {
    /// Settles each workspace an earlier hub left part way through being archived or restored, once this one is
    /// serving, so a worker that's slow to answer keeps no client waiting. Until then its record says it's being
    /// archived or restored, as it did, and nothing else can be asked of it. Once every move has settled, tidies any
    /// stale archive bundles.
    pub(crate) async fn settle_moves(self: Arc<Self>, left: Vec<(WorkspaceId, Move)>) {
        for (id, moving) in left {
            match self.settle_move(&id, moving).await {
                Ok(workspace) => {
                    let status = workspace.status;
                    info!(
                        name: "archive.settled",
                        workspace_id = %workspace.id,
                        ?status,
                        "Settled a workspace the last run was moving"
                    );
                }
                Err(e) => {
                    warn!(
                        name: "archive.unsure",
                        workspace_id = %id,
                        ?moving,
                        error = e.message,
                        "Couldn't tell how far the last run got, so putting it back"
                    );
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
            Err(e) => {
                return warn!(
                    name: "archive.settled_unread",
                    workspace_id = %id,
                    error = e.message,
                    "Couldn't read a workspace that was settled"
                );
            }
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
                Err(e) => {
                    warn!(
                        name: "archive.worker_not_stopped",
                        workspace_id = %id,
                        error = e.message,
                        "Couldn't stop the worker of a workspace settled as archived"
                    );
                }
            },
        }
    }

    /// Records what the workspace's worker finds: a worktree that's there is a ready workspace's, and one that isn't
    /// is an archived one's, whichever the earlier hub was doing. Fails if the worker can't say, such as one that's
    /// gone.
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
                self.record_archived(id, self.checked_out_branch(id)).await
            }
            (Move::Restore, true) => self.record_restored(id, &link).await,
        }
    }
}
