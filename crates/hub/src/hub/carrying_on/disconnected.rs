//! A workspace whose worker an earlier hub left, and that hasn't dialed this hub: waited for, not replaced.

use super::super::Hub;
use super::LeftAtWork;
use super::recovery::{Carried, interrupt};
use parking_lot::Mutex;
use protocol::model::{Seq, SessionId, WorkspaceId, WorkspaceStatus};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tracing::warn;

impl Hub {
    /// Waits for the worker an earlier hub left to dial this one, for as long as it takes, and then carries on with it
    /// as [`Hub::carry_on`] does one that dialed within `before_serving`. It stops once the worker is carried on with,
    /// or its provider says it's gone, as for one that never came, or the hub stops. Nothing here ends the worker
    /// because it's quiet, so it isn't replaced meanwhile, as [`Hub::ensure_worker`] says. Each look waits
    /// `before_serving` for the dial, which only paces how often the provider is asked and the hub's stopping noticed.
    pub(super) async fn wait_for_worker(self: Arc<Self>, workspace_id: WorkspaceId) {
        loop {
            if self.shutting_down.load(Ordering::SeqCst) {
                break;
            }
            let read = self.blocking(LeftAtWork::read).await;
            let id = workspace_id.clone();
            let Ok(LeftAtWork { mut sessions, workspaces, untaken }) = read else {
                warn!(name: "recovery.unread", %workspace_id, "Couldn't read what is left at work to wait for a worker");
                break;
            };
            let being_moved = workspaces.iter().any(|workspace| {
                workspace.id == id
                    && matches!(workspace.status, WorkspaceStatus::Archiving | WorkspaceStatus::Restoring)
            });
            let at_work = sessions.remove(&id).unwrap_or_default();
            let untaken = Arc::new(Mutex::new(untaken));
            if self.carry_on_workspace(&id, at_work, being_moved, untaken).await == Carried::Done {
                break;
            }
        }
        self.disconnected.lock().remove(&workspace_id);
    }

    /// Interrupts the session `id`, whose agent isn't on the worker, with `untaken` the prompts its agent hadn't taken.
    /// Written to the store as the hub starts, when no session is live, and through the session once it is, as for a
    /// worker that dials late, since a client following the session makes it.
    pub(super) async fn interrupt_session(&self, id: SessionId, untaken: Vec<Seq>) {
        let live = self.sessions.lock().live.get(&id).cloned();
        let Some(live) = live else {
            return self.write(move |store| interrupt(store, &id, untaken)).await;
        };
        let waiting = {
            let id = id.clone();
            self.blocking(move |store| Ok(store.sessions_waiting()?.contains(&id))).await.unwrap_or(false)
        };
        live.interrupted_by_restart(waiting).await;
    }

    /// Ends the worker of each workspace that was waited for, as the hub stops for good, since a stop says nothing
    /// carries on with them. The hub has no connection to them, so its providers end them.
    pub(in super::super) async fn end_disconnected_workers(&self) {
        let waiting: Vec<WorkspaceId> = self.disconnected.lock().iter().cloned().collect();
        for workspace_id in waiting {
            let provider = self.workspace(&workspace_id).await.and_then(|workspace| {
                self.provider(&workspace.provider).map(|provider| (provider, workspace.instance_id))
            });
            match provider {
                Ok((provider, instance_id)) => {
                    self.clear_instance(provider.as_ref(), &instance_id).await;
                }
                Err(e) => {
                    warn!(
                        name: "worker.disconnected_unended",
                        %workspace_id,
                        error = e.message,
                        "Couldn't end a worker the hub never reached"
                    );
                }
            }
        }
    }
}
