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
            warn!(
                name: "changes.lines_unkept",
                workspace_id = %id,
                error = e.message,
                "Couldn't keep the lines the workspace adds and removes"
            );
        }
        if let Err(e) = self.observation_read::<WorkspaceChanges>(id.clone(), summary, read_at_ms).await {
            warn!(
                name: "changes.files_unkept",
                workspace_id = %id,
                error = e.message,
                "Couldn't keep the files the workspace changes"
            );
        }
        // Removed or archived while it was read, after that forgot what was kept, so what this kept goes too. One
        // removed or archived after this looks forgets it after this read kept it.
        match self.workspace(id).await {
            Err(e) if e.code == ErrorCode::NotFound => self.workspace_observations_gone(id).await,
            Ok(workspace) if workspace.status == WorkspaceStatus::Archived => self.changes_forgotten(id).await,
            Ok(_) | Err(_) => {}
        }
    }

    /// Forgets the workspace's changes, its lines and its files, as when it's archived, since no client draws either
    /// for a workspace whose worktree is gone, and bringing it back reads them again once its worker says its
    /// branches. What GitHub says of it stays, since Home draws an archived workspace's pull request.
    pub(crate) async fn changes_forgotten(&self, id: &WorkspaceId) {
        if let Err(e) = self.observation_gone::<WorkspaceLines>(id.clone()).await {
            warn!(
                name: "changes.lines_unforgotten",
                workspace_id = %id,
                error = e.message,
                "Couldn't forget the lines a workspace added and removed"
            );
        }
        if let Err(e) = self.observation_gone::<WorkspaceChanges>(id.clone()).await {
            warn!(
                name: "changes.files_unforgotten",
                workspace_id = %id,
                error = e.message,
                "Couldn't forget the files a workspace changed"
            );
        }
    }
}
