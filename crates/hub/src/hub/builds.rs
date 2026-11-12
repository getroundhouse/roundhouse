//! Moving each workspace's worker onto this hub's build. A worker an earlier hub left running can be another build,
//! as after an update, and this hub carries on with it while anything in its workspace is at work, so an update never
//! cuts an agent short. Once nothing is, it stops that worker, and the next request for the workspace starts its own.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use protocol::model::WorkspaceId;
use tracing::{info, warn};

use super::Hub;
use crate::worker_link::WorkerLink;

/// How often the hub looks at a workspace whose worker is another build. Two looks in a row have to find nothing at
/// work, so a moment with nothing at work, as just before an agent picks up again by itself once its work in the
/// background is done, isn't taken for the workspace being idle.
const LOOK_EVERY: Duration = Duration::from_secs(5);

impl Hub {
    /// Replaces `link`, the workspace's worker, which is another build, once two looks in a row find nothing in the
    /// workspace at work. Gives up once the worker has gone, or been replaced, or the hub is stopping.
    pub(super) fn replace_once_idle(self: &Arc<Self>, workspace_id: WorkspaceId, link: Arc<WorkerLink>) {
        let hub = self.clone();
        tokio::spawn(async move {
            let mut idle_before = false;
            loop {
                tokio::select! {
                    () = tokio::time::sleep(LOOK_EVERY) => {}
                    () = link.wait_closed() => return,
                }
                let idle = hub.nothing_at_work_in(&workspace_id, &link);
                if idle && idle_before && hub.replace_if_idle(&workspace_id, &link).await {
                    return;
                }
                idle_before = idle;
            }
        });
    }

    /// Stops the workspace's worker, so the next request for the workspace starts this hub's build, unless something
    /// in the workspace is at work. Under the workspace's gate, so the next worker can't start until this one has
    /// stopped. Whether there's anything more to wait for: not once it's stopped, or the hub has replaced it already,
    /// or is stopping.
    ///
    /// Something that starts work records it before it asks for the worker, as a prompt is recorded before it's sent,
    /// so once nothing is at work, whatever wants the worker next finds it gone from the map, and waits for the gate.
    async fn replace_if_idle(&self, workspace_id: &WorkspaceId, link: &Arc<WorkerLink>) -> bool {
        let workspace = match self.workspace(workspace_id).await {
            Ok(workspace) => workspace,
            // Looked at again.
            Err(e) => {
                warn!(%workspace_id, "replacing a worker from another build: {}", e.message);
                return false;
            }
        };
        let gate = self.worker_gate(workspace_id);
        let mut cleared = gate.lock().await;
        if !self.nothing_at_work_in(workspace_id, link) {
            return false;
        }
        {
            let mut workers = self.workers.lock();
            // A hub that's stopping stops or leaves it, as it does every worker.
            if !link.is_current(&workers, workspace_id) || self.shutting_down.load(Ordering::SeqCst) {
                return true;
            }
            workers.remove(workspace_id);
        }
        // So each agent's exit, which stopping it brings, is of an agent its session has let go of, and changes nothing
        // even if a prompt is recorded first.
        for session in self.sessions.lock().live.values().filter(|session| &session.workspace_id == workspace_id) {
            session.let_go_of_agent_on(link.id);
        }
        info!(%workspace_id, build = link.build(), "stopping a worker from another build, with nothing at work");
        self.stop_and_clear(&workspace, link, &mut cleared, "replacing a worker from another build").await;
        true
    }

    /// Whether nothing in the workspace would be cut short by stopping its worker: no session at work, which covers one
    /// waiting on an approval or with work in the background, no setup script, and no call to the worker under way,
    /// such as a read or a title being written.
    fn nothing_at_work_in(&self, workspace_id: &WorkspaceId, link: &WorkerLink) -> bool {
        let session_at_work = self
            .sessions
            .lock()
            .live
            .values()
            .any(|session| &session.workspace_id == workspace_id && session.status().is_active());
        !session_at_work && !self.setup_gates.lock().contains_key(workspace_id) && link.calls_under_way() == 0
    }
}

#[cfg(test)]
mod tests;
