//! Reading a workspace's lines from its worker, and keeping what it says.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::ErrorCode;
use protocol::model::WorkspaceId;
use protocol::worker::SummarizeChanges;
use tracing::{debug, warn};

use crate::hub::Hub;
use crate::hub::observations::WorkspaceLines;
use crate::jobs::{JobKind, Limits, step, uncut};
use crate::store::now_ms;
use crate::worker_link::WorkerLink;

/// How long a read has. Its work asks the worker to sum up its worktree's changes, which git counts in a second or two
/// even for a large worktree, so a worker that takes this long is stuck, and a call left waiting on it would keep its
/// workspace from being idle, so its worker from being replaced. Its after-step has nothing to put back, and waits only
/// for what the read keeps, which runs uncut and waits on nothing outside the hub.
const READ: Limits = Limits { work: Duration::from_secs(30), after: Duration::from_secs(10) };

impl Hub {
    /// Reads the workspace's lines again, since a turn has ended in it, from the worker it has, if it has one, since a
    /// read nobody waits on starts none.
    pub(crate) fn read_lines_after_turn(self: &Arc<Self>, id: &WorkspaceId) {
        if let Some(link) = self.existing_worker(id) {
            self.read_lines(id, &link);
        }
    }

    /// Reads the workspace's lines from `link`, its worker, as a job, so whatever said they may have changed doesn't
    /// wait for the worker. One asked for while a read of the workspace runs runs once more after it, however often it
    /// was asked for, from the worker the last one asked named, since the read under way may have begun before the
    /// change. What it reads is kept as of when the read began, and a read that fails leaves what's kept as it was.
    /// Not while its worker watches its worktree, which sends them as they change.
    pub(crate) fn read_lines(self: &Arc<Self>, id: &WorkspaceId, link: &Arc<WorkerLink>) {
        if self.observation_is_kept_current::<WorkspaceLines>(id) {
            return;
        }
        let (hub, link, of) = (self.clone(), link.clone(), id.clone());
        let work = async move { hub.read_lines_once(&of, &link).await };
        // A read cut short leaves what's kept as it was, as one that fails does.
        let after = |_| std::future::ready(());
        if let Err(not) = self.line_reads.start(JobKind::ReadWorkspaceChanges, id.clone(), READ, work, after) {
            debug!(%id, ?not, "a read of the workspace's changes didn't start");
        }
    }

    /// Reads the changes of the workspace `id` from `link`, if the workspace is ready, since one being made has no
    /// worktree to read yet, and an archived one's is gone, and keeps what the worker says, as [`Hub::changes_read`]
    /// does, uncut, so the two observations it fills are as of the same read.
    async fn read_lines_once(self: &Arc<Self>, id: &WorkspaceId, link: &WorkerLink) {
        step("reading the workspace's record");
        let Ok(ready) = self.ready_workspace(id).await else { return };
        let read_at_ms = now_ms();
        let asked = SummarizeChanges { path: ready.path().to_owned(), base_ref: ready.workspace().base_ref.clone() };
        step(format!("asking {link} to sum up its changes"));
        let summary = match link.call(asked).await {
            Ok(summary) => summary,
            // A worker from before the request, which the hub replaces once nothing in its workspace is at work.
            Err(e) if e.code == ErrorCode::Unsupported => {
                return debug!(%id, "the worker can't sum up its changes: {}", e.message);
            }
            Err(e) => return warn!(%id, "reading the lines the workspace adds and removes: {}", e.message),
        };
        let (hub, id) = (self.clone(), id.clone());
        uncut(async move { hub.changes_read(&id, summary, read_at_ms).await }).await;
    }
}
