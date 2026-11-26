//! Keeping what a summary of a workspace's changes says, as the two observations it fills.

use protocol::model::{ChangeSummary, WorkspaceId};

use crate::hub::Hub;
use crate::hub::observations::{WorkerWord, WorkspaceChanges, WorkspaceLines};

impl Hub {
    /// Keeps `summary`, the workspace's changes as its worker summed them up at `read_at_ms`, by the hub's clock, as
    /// both observations it fills: its totals as the workspace's lines, at the hub's level, and the summary whole as
    /// its changed files, at the workspace's. Every summary the hub gets goes through here, so the two are as of the
    /// same read, and only while the worker whose word `word` is is still the workspace's, and its worktree is there,
    /// so not once the workspace is removed or archived. A failure to keep either, which the store logs, leaves what
    /// was kept of it as it was.
    pub(crate) async fn changes_read(
        &self,
        id: &WorkspaceId,
        word: &WorkerWord<'_>,
        summary: ChangeSummary,
        read_at_ms: i64,
    ) {
        word.read::<WorkspaceLines>(id.clone(), summary.lines, read_at_ms).await;
        word.read::<WorkspaceChanges>(id.clone(), summary, read_at_ms).await;
    }
}
