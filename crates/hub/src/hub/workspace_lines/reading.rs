//! Reading a workspace's lines from its worker, and keeping what it says.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::ErrorCode;
use protocol::model::WorkspaceId;
use protocol::worker::SummarizeChanges;
use tracing::{debug, warn};

use super::reads::Reading;
use crate::hub::Hub;
use crate::hub::observations::WorkspaceLines;
use crate::store::now_ms;
use crate::worker_link::WorkerLink;

/// How long a worker has to sum up its worktree's changes. Git counts even a large worktree's in a second or two, so
/// one that takes this long is stuck, and a call left waiting on it would keep its workspace from being idle, so its
/// worker from being replaced.
const READ_LIMIT: Duration = Duration::from_secs(30);

impl Hub {
    /// Reads the workspace's lines again, since a turn has ended in it, from the worker it has, if it has one, since a
    /// read nobody waits on starts none.
    pub(crate) fn read_lines_after_turn(self: &Arc<Self>, id: &WorkspaceId) {
        if let Some(link) = self.existing_worker(id) {
            self.read_lines(id, &link);
        }
    }

    /// Reads the workspace's lines from `link`, its worker, on a task of its own, so whatever said they may have
    /// changed doesn't wait for the worker, as [`super::Reads`] says. What it reads is kept as of when the read
    /// began, and a read that fails leaves what's kept as it was.
    pub(crate) fn read_lines(self: &Arc<Self>, id: &WorkspaceId, link: &Arc<WorkerLink>) {
        let Some(mut reading) = self.line_reads.begin(id, link) else { return };
        let (hub, mut link) = (self.clone(), link.clone());
        tokio::spawn(async move {
            loop {
                hub.read_lines_once(&reading, &link).await;
                let Some(again) = reading.again() else { break };
                link = again;
            }
        });
    }

    /// Reads the lines of the workspace `reading` is of from `link`, if the workspace is ready, since one being made has
    /// no worktree to read yet, and an archived one's is gone, and keeps what the worker says.
    async fn read_lines_once(&self, reading: &Reading, link: &WorkerLink) {
        let id = reading.id();
        let Ok(ready) = self.ready_workspace(id).await else { return };
        let read_at_ms = now_ms();
        let asked = SummarizeChanges { path: ready.path().to_owned(), base_ref: ready.workspace().base_ref.clone() };
        let summary = match tokio::time::timeout(READ_LIMIT, link.call(asked)).await {
            Ok(Ok(summary)) => summary,
            // A worker from before the request, which the hub replaces once nothing in its workspace is at work.
            Ok(Err(e)) if e.code == ErrorCode::Unsupported => {
                return debug!(%id, "the worker can't sum up its changes: {}", e.message);
            }
            Ok(Err(e)) => return warn!(%id, "reading the lines the workspace adds and removes: {}", e.message),
            Err(_) => return warn!(%id, "the worker didn't sum up its changes within {READ_LIMIT:?}"),
        };
        if let Err(e) = self.observation_read::<WorkspaceLines>(id.clone(), summary.lines, read_at_ms).await {
            return warn!(%id, "keeping the lines the workspace adds and removes: {}", e.message);
        }
        // Removed while it was read, after its removal forgot what was kept, so what this kept goes too. One removed
        // after this looks is forgotten by its removal, after this read kept it.
        if self.workspace(id).await.is_err_and(|e| e.code == ErrorCode::NotFound)
            && let Err(e) = self.observation_gone::<WorkspaceLines>(id.clone()).await
        {
            warn!(%id, "forgetting the lines of a workspace that's gone: {}", e.message);
        }
    }
}
