//! Keeping what a summary of a workspace's changes says, as the two observations it fills.

use protocol::client::ErrorCode;
use protocol::model::{ChangeSummary, WorkspaceId, WorkspaceStatus};
use tracing::warn;

use crate::hub::Hub;
use crate::hub::observations::{WorkspaceChanges, WorkspaceLines};

impl Hub {
    /// Keeps `summary`, the workspace's changes as its worker summed them up at `read_at_ms`, by the hub's clock, as
    /// both observations it fills: its totals as the workspace's lines, at the hub's level, and the summary whole as
    /// its changed files, at the workspace's. Every summary the hub gets goes through here, so the two are as of the
    /// same read. A failure to keep either is only logged, as what was kept of it stays as it was.
    pub(crate) async fn changes_read(&self, id: &WorkspaceId, summary: ChangeSummary, read_at_ms: i64) {
        if let Err(e) = self.observation_read::<WorkspaceLines>(id.clone(), summary.lines, read_at_ms).await {
            warn!(%id, "keeping the lines the workspace adds and removes: {}", e.message);
        }
        if let Err(e) = self.observation_read::<WorkspaceChanges>(id.clone(), summary, read_at_ms).await {
            warn!(%id, "keeping the files the workspace changes: {}", e.message);
        }
        // Removed or archived while it was read, after that forgot what was kept, so what this kept goes too. One
        // removed or archived after this looks forgets it after this read kept it.
        match self.workspace(id).await {
            Err(e) if e.code == ErrorCode::NotFound => self.workspace_observations_gone(id).await,
            Ok(workspace) if workspace.status == WorkspaceStatus::Archived => self.changes_forgotten(id).await,
            Ok(_) | Err(_) => {}
        }
    }

    /// Forgets every observation of the workspace, which is gone.
    pub(crate) async fn workspace_observations_gone(&self, id: &WorkspaceId) {
        if let Err(e) = self.observation_gone::<WorkspaceLines>(id.clone()).await {
            warn!(%id, "forgetting the lines of a workspace that's gone: {}", e.message);
        }
        self.changes_forgotten(id).await;
        self.github_observations_gone(id).await;
    }

    /// Forgets the files the workspace changes, as when it's archived, since no pane draws an archived workspace's
    /// changes from a worktree that's gone. Its lines are kept, as the sidebar and Home draw them.
    pub(crate) async fn changes_forgotten(&self, id: &WorkspaceId) {
        if let Err(e) = self.observation_gone::<WorkspaceChanges>(id.clone()).await {
            warn!(%id, "forgetting the files a workspace changed: {}", e.message);
        }
    }
}
