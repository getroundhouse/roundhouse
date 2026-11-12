//! Moving each workspace's worker onto this hub's build. A worker an earlier hub left running can be another build,
//! as after an update, and this hub carries on with it while anything in its workspace is at work, so an update never
//! cuts an agent short. Once nothing has been for a while, it stops that worker, and the next request for the
//! workspace starts its own.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use protocol::model::WorkspaceId;
use tracing::{info, warn};

use super::Hub;
use crate::worker_link::WorkerLink;

/// How long nothing in the workspace has to be at work, and nothing has to use its worker, before the worker is
/// replaced. An agent picks up again by itself once its work in the background is done, with nothing at work in
/// between, and some of what the hub does makes one call after another, such as reading a pull request after a turn.
const QUIET: Duration = Duration::from_secs(10);

/// How long to wait before looking again when only a call to the worker was under way, which says nothing when it's
/// done.
const LOOK_AGAIN: Duration = Duration::from_secs(1);

/// What looking at a workspace found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Looked {
    /// Something in it is at work, so it's looked at again once what's at work changes.
    AtWork,
    /// Nothing is at work, but a call to the worker is under way, such as a read or a title being written.
    CallUnderWay,
    /// Nothing is at work, and the worker had been used this many times, as [`WorkerLink::uses`] says.
    Idle(u64),
}

impl Hub {
    /// Replaces `link`, the workspace's worker, which is another build, once nothing in the workspace has been at work,
    /// and nothing has used the worker, for [`QUIET`]. Gives up once the worker has gone, or been replaced, or the hub
    /// is stopping.
    pub(super) fn replace_once_idle(self: &Arc<Self>, workspace_id: WorkspaceId, link: Arc<WorkerLink>) {
        let hub = self.clone();
        tokio::spawn(async move {
            let mut changes = hub.work_changes.subscribe();
            loop {
                // Before looking, so a change while it looks is waited for.
                changes.mark_unchanged();
                let (wait, idle) = match hub.at_work_in(&workspace_id, &link) {
                    Looked::AtWork => {
                        tokio::select! {
                            _ = changes.changed() => continue,
                            () = link.wait_closed() => return,
                        }
                    }
                    Looked::CallUnderWay => (LOOK_AGAIN, None),
                    Looked::Idle(uses) => (QUIET, Some(uses)),
                };
                // Not woken by a change meanwhile, which can be another workspace's. Work in this one uses its worker,
                // which is what's looked for once the wait is over.
                tokio::select! {
                    () = tokio::time::sleep(wait) => {}
                    () = link.wait_closed() => return,
                }
                if let Some(uses) = idle
                    && hub.replace_if_unused(&workspace_id, &link, uses).await
                {
                    return;
                }
            }
        });
    }

    /// Stops the workspace's worker, so the next request for the workspace starts this hub's build, unless something
    /// in the workspace is at work, or something has used the worker since it had been used `uses` times. Under the
    /// workspace's gate, so the next worker can't start until this one has stopped. Whether there's anything more to
    /// wait for: not once it's stopped, or the hub has replaced it already, or is stopping.
    ///
    /// Something that starts work records it before it asks for the worker, as a prompt is recorded before it's sent,
    /// and the worker is counted each time it's handed out, so once nothing is at work and nothing has used it since,
    /// whatever wants it next finds it gone from the map, and waits for the gate.
    async fn replace_if_unused(&self, workspace_id: &WorkspaceId, link: &Arc<WorkerLink>, uses: u64) -> bool {
        let workspace = match self.workspace(workspace_id).await {
            Ok(workspace) => workspace,
            // Looked at again, as if it had been used.
            Err(e) => {
                warn!(%workspace_id, "replacing a worker from another build: {}", e.message);
                return false;
            }
        };
        let gate = self.worker_gate(workspace_id);
        let mut cleared = gate.lock().await;
        if self.at_work_in(workspace_id, link) != Looked::Idle(uses) {
            return false;
        }
        {
            let mut workers = self.workers.lock();
            // A hub that's stopping stops or leaves it, as it does every worker.
            if !link.is_current(&workers, workspace_id) || self.shutting_down.load(Ordering::SeqCst) {
                return true;
            }
            // Handed out since it looked, for work that may not have started yet.
            if link.uses() != uses {
                return false;
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

    /// Whether anything in the workspace would be cut short by stopping its worker: a session at work, which covers one
    /// waiting on an approval or with work in the background, a setup script, or a call to the worker. How many times
    /// the worker had been used is read first, so whatever uses it once this has looked is seen to have.
    fn at_work_in(&self, workspace_id: &WorkspaceId, link: &WorkerLink) -> Looked {
        let uses = link.uses();
        let session_at_work = self
            .sessions
            .lock()
            .live
            .values()
            .any(|session| &session.workspace_id == workspace_id && session.status().is_active());
        if session_at_work || self.setup_gates.lock().contains_key(workspace_id) {
            Looked::AtWork
        } else if link.calls_under_way() > 0 {
            Looked::CallUnderWay
        } else {
            Looked::Idle(uses)
        }
    }
}

#[cfg(test)]
mod tests;
