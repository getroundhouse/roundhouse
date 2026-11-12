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

/// How long to wait before looking again when only a call to the worker was under way, which says nothing when it's
/// done.
const LOOK_AGAIN: Duration = Duration::from_secs(1);

/// What looking at a workspace found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Looked {
    /// Its worker has gone, or been replaced, or the hub is stopping, so there's nothing more to wait for.
    Done,
    /// Something in it is at work, so it's looked at again once what's at work changes.
    AtWork,
    /// Nothing is at work, but a call to the worker is under way, such as a read or a title being written.
    CallUnderWay,
    Idle,
}

impl Hub {
    /// Replaces `link`, the workspace's worker, which is another build, once nothing in the workspace is at work,
    /// looking again each time what's at work changes. Until it's gone, as when the hub stops.
    pub(super) fn replace_once_idle(self: &Arc<Self>, workspace_id: WorkspaceId, link: Arc<WorkerLink>) {
        let hub = self.clone();
        tokio::spawn(async move {
            let mut changes = hub.work_changes.subscribe();
            loop {
                // Before looking, so a change while it looks is waited for.
                changes.borrow_and_update();
                let looked = hub.replace_if_idle(&workspace_id, &link).await;
                if looked == Looked::Done {
                    return;
                }
                let look_again = async {
                    if looked == Looked::CallUnderWay {
                        tokio::time::sleep(LOOK_AGAIN).await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                };
                tokio::select! {
                    heard = changes.changed() => if heard.is_err() { return },
                    () = look_again => {}
                    () = link.wait_closed() => return,
                }
            }
        });
    }

    /// Stops the workspace's worker if nothing in the workspace is at work, so the next request for it starts this
    /// hub's build. Under the workspace's gate, and with the worker handed to nothing from before it looks, so what
    /// starts work once it has looked waits for the gate, and the worker that replaces this one. Something that starts
    /// work is recorded first, as a prompt is before it's sent, so whatever started before is seen.
    async fn replace_if_idle(&self, workspace_id: &WorkspaceId, link: &Arc<WorkerLink>) -> Looked {
        let workspace = match self.workspace(workspace_id).await {
            Ok(workspace) => workspace,
            Err(e) => {
                warn!(%workspace_id, "replacing a worker from another build: {}", e.message);
                return Looked::Done;
            }
        };
        let gate = self.worker_gate(workspace_id);
        let mut cleared = gate.lock().await;
        {
            let workers = self.workers.lock();
            // A hub that's stopping stops or leaves it, as it does every worker.
            if !link.is_current(&workers, workspace_id) || self.shutting_down.load(Ordering::SeqCst) {
                return Looked::Done;
            }
            link.set_replacing(true);
        }
        let looked = self.at_work_in(workspace_id, link);
        if looked != Looked::Idle {
            let _workers = self.workers.lock();
            link.set_replacing(false);
            return looked;
        }
        self.workers.lock().remove(workspace_id);
        info!(%workspace_id, build = link.build(), "stopping a worker from another build, with nothing at work");
        self.stop_and_clear(&workspace, link, &mut cleared, "replacing a worker from another build").await;
        Looked::Done
    }

    /// Whether anything in the workspace would be cut short by stopping its worker: a session at work, which covers one
    /// waiting on an approval or with work in the background, a setup script, or a call to the worker.
    fn at_work_in(&self, workspace_id: &WorkspaceId, link: &WorkerLink) -> Looked {
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
            Looked::Idle
        }
    }
}
