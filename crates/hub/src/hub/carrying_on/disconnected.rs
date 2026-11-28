//! A workspace whose worker an earlier hub left, and that hasn't dialed this hub: waited for, not replaced.

use super::super::Hub;
use super::super::observations::Keeper;
use super::LeftAtWork;
use super::recovery::{Carried, interrupt};
use crate::spans;
use crate::worker_link::WorkerLink;
use parking_lot::Mutex;
use protocol::event::ExitReason;
use protocol::model::{InstanceId, Seq, SessionId, WorkspaceId, WorkspaceStatus};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::Notify;
use tokio::time::Instant;
use tracing::{Instrument as _, warn};

/// Why a workspace's worker can't be reached.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cause {
    /// A hub that was running left it, and this hub, as it started, found it hadn't dialed.
    LeftByEarlierHub,
    /// Its connection to this hub ended with no goodbye.
    ConnectionDropped,
}

/// The workspaces whose worker the hub can't reach, which it waits for, and since when. A worker's connection is how it
/// talks to its hub, not proof that it's alive, so a workspace is here from when a connection ends with no goodbye, or a
/// starting hub finds no worker that an earlier hub left, until the worker dials, its provider says it's gone, or the
/// hub stops. Only in memory, since it's the hub that's running that can't reach the worker, and the next works that
/// out again.
#[derive(Default)]
pub(crate) struct Disconnected {
    since: Mutex<HashMap<WorkspaceId, (Instant, Cause)>>,
    /// Told each time a workspace leaves, for what waits for its worker.
    left: Notify,
}

impl Disconnected {
    /// Says the workspace's worker can't be reached, from now, unless it already was. Whether it newly is.
    pub(crate) fn mark(&self, id: &WorkspaceId, cause: Cause) -> bool {
        let mut since = self.since.lock();
        if since.contains_key(id) {
            return false;
        }
        since.insert(id.clone(), (Instant::now(), cause));
        true
    }

    /// Whether its worker was connected to this hub until its connection dropped, which a worker found gone was, so
    /// its agents were lost with it, where one an earlier hub left was never this hub's to lose.
    pub(crate) fn dropped(&self, id: &WorkspaceId) -> bool {
        self.since.lock().get(id).is_some_and(|(_, cause)| *cause == Cause::ConnectionDropped)
    }

    pub(crate) fn clear(&self, id: &WorkspaceId) {
        self.since.lock().remove(id);
        self.left.notify_waiters();
    }

    pub(crate) fn contains(&self, id: &WorkspaceId) -> bool {
        self.since.lock().contains_key(id)
    }

    /// How long ago the workspace was marked, if it is.
    pub(crate) fn for_how_long(&self, id: &WorkspaceId) -> Option<std::time::Duration> {
        self.since.lock().get(id).map(|(since, _)| since.elapsed())
    }

    pub(crate) fn ids(&self) -> Vec<WorkspaceId> {
        self.since.lock().keys().cloned().collect()
    }

    pub(crate) fn any(&self) -> bool {
        !self.since.lock().is_empty()
    }

    /// Returns once the workspace isn't disconnected, which is at once if it isn't.
    pub(crate) async fn until_connected(&self, id: &WorkspaceId) {
        loop {
            // Before looking, so a workspace that leaves in between is seen.
            let left = self.left.notified();
            tokio::pin!(left);
            left.as_mut().enable();
            if !self.contains(id) {
                return;
            }
            left.await;
        }
    }
}

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
        self.disconnected.clear(&workspace_id);
    }

    /// Called when a dialed worker's connection ends with no goodbye. The worker may be running yet, with its agents,
    /// so it isn't ended, and its sessions aren't told their agents are lost. The workspace is disconnected until the
    /// worker dials again, as for one an earlier hub left, which is then carried on with, or its provider says it's
    /// gone. What was bound to the connection goes, and the next one gives it back.
    ///
    /// Boxed, since waiting opens a link, whose reading may end in a worker disconnected again.
    pub(crate) fn worker_disconnected<'a>(
        self: &'a Arc<Self>,
        workspace_id: &'a WorkspaceId,
        link: &'a Arc<WorkerLink>,
        instance_id: &'a InstanceId,
    ) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(self.disconnecting(workspace_id, link, instance_id))
    }

    async fn disconnecting(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
        link: &Arc<WorkerLink>,
        instance_id: &InstanceId,
    ) {
        // The connection may still be open, as when the worker dialed again before the hub's own timer ran out.
        link.kill();
        let waiting = {
            let mut workers = self.workers.lock();
            // One that's been replaced or stopped is no concern of its workspace's, which has another.
            if !link.is_current(&workers, workspace_id) || self.shutting_down.load(Ordering::SeqCst) {
                return;
            }
            // Before it leaves the map, so a worker wanted meanwhile isn't started on the instance.
            let newly = self.disconnected.mark(workspace_id, Cause::ConnectionDropped);
            workers.remove(workspace_id);
            newly
        };
        warn!(
            name: "worker.disconnected",
            %workspace_id,
            %instance_id,
            "A worker's connection ended with no goodbye, so its workspace is disconnected until it dials again"
        );
        // What's on the instance is no longer known to be what the hub holds, so whatever clears it next, as for a worker
        // found gone, ends the worker and its agents.
        *self.worker_gate(workspace_id).lock().await = false;
        self.terminals_lost(link.id).await;
        self.changes_watcher_gone(workspace_id, link).await;
        self.keeper_went(Keeper::Worker(link.id)).await;
        if waiting {
            let span = spans::workspace(workspace_id);
            tokio::spawn(self.clone().wait_for_worker(workspace_id.clone()).instrument(span));
        }
    }

    /// Returns once the workspace's worker can be reached, which is at once if it can be, for what's to be sent to its
    /// agent.
    pub(crate) async fn wait_until_connected(&self, workspace_id: &WorkspaceId) {
        self.disconnected.until_connected(workspace_id).await;
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
        // A worker that was connected to this hub went with its agents, as one found gone does, where one an earlier
        // hub left is only what that hub's restart cut off.
        let reason =
            if self.disconnected.dropped(&live.workspace_id) { ExitReason::WorkerLost } else { ExitReason::HubRestart };
        live.interrupted_by_restart(waiting, reason).await;
    }

    /// Ends the worker of each workspace that was waited for, as the hub stops for good, since a stop says nothing
    /// carries on with them. The hub has no connection to them, so its providers end them.
    pub(in super::super) async fn end_disconnected_workers(&self) {
        let waiting = self.disconnected.ids();
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
