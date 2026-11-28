//! A workspace whose worker the hub can't reach, because an earlier hub left it and it hasn't dialed this one, or because
//! its connection to this one dropped: waited for, not replaced, and taken up when it dials.

use super::super::Hub;
use super::super::observations::Keeper;
use super::LeftAtWork;
use super::disconnected_workspaces::Cause;
use super::recovery::{Carried, interrupt};
use crate::spans;
use crate::worker_link::WorkerLink;
use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::ExitReason;
use protocol::model::{InstanceId, Seq, SessionId, WorkspaceId, WorkspaceStatus};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tracing::{Instrument as _, warn};

pub(in super::super) fn not_connected() -> ApiError {
    ApiError::new(
        ErrorCode::Unavailable,
        "the workspace's worker isn't connected to the hub, and is waited for rather than replaced",
    )
}

impl Hub {
    /// Waits for the worker of a disconnected workspace to dial this hub, for as long as it takes, and then carries on
    /// with it as [`Hub::carry_on`] does one that dialed within `before_serving`. It stops once the worker is carried on
    /// with, or its provider says it's gone, as for one that never came, or the hub stops. Nothing here ends the worker
    /// because it's quiet, so it isn't replaced meanwhile, as [`Hub::ensure_worker`] says. Each look waits
    /// `before_serving` for the dial, which only paces how often the hub's stopping is noticed: how often the provider is
    /// asked is the workspace's [`Disconnected`](super::Disconnected) entry's to say. A worker that dials and drops
    /// again as it's taken up is waited for again.
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
            self.disconnected.begin_attempt(&id);
            if self.carry_on_workspace(&id, at_work, being_moved, untaken).await == Carried::Done
                && self.disconnected.clear_unless_dropped_again(&id)
            {
                return;
            }
        }
        self.disconnected.clear(&workspace_id);
    }

    /// Says that a dialed worker's connection ended with no goodbye, before anything else sees it ended, if the link is
    /// still its workspace's. Whether the workspace is newly disconnected by it, or `None` if the link has been replaced,
    /// which has nothing to do with it. One that was disconnected already, since its worker was being taken up, is told
    /// that it dropped again, so that isn't the end of waiting for it.
    pub(crate) fn begin_disconnect(
        &self,
        workspace_id: &WorkspaceId,
        link: &Arc<WorkerLink>,
        instance_id: &InstanceId,
    ) -> Option<bool> {
        let workers = self.workers.lock();
        if !link.is_current(&workers, workspace_id) || self.shutting_down.load(Ordering::SeqCst) {
            return None;
        }
        // Before it leaves the map, so a worker wanted meanwhile isn't started on the instance.
        let newly = self.disconnected.mark(workspace_id, instance_id, Cause::ConnectionDropped, false, Some(link.id));
        if !newly {
            self.disconnected.dropped_again(workspace_id, link.id);
        }
        Some(newly)
    }

    /// Called when a dialed worker's connection ends with no goodbye, once [`Hub::begin_disconnect`] has said so. The
    /// worker may be running yet, with its agents, so it isn't ended, and its sessions aren't told their agents are
    /// lost. The workspace is disconnected until the worker dials again, as for one an earlier hub left, which is then
    /// carried on with, or its provider says it's gone. What was bound to the connection goes, and the next one gives
    /// it back. `newly` is whether this disconnected the workspace, which then has a wait started for its worker.
    ///
    /// Boxed, since waiting opens a link, whose reading may end in a worker disconnected again.
    pub(crate) fn worker_disconnected<'a>(
        self: &'a Arc<Self>,
        workspace_id: &'a WorkspaceId,
        link: &'a Arc<WorkerLink>,
        instance_id: &'a InstanceId,
        newly: bool,
    ) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(self.disconnecting(workspace_id, link, instance_id, newly))
    }

    async fn disconnecting(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
        link: &Arc<WorkerLink>,
        instance_id: &InstanceId,
        newly: bool,
    ) {
        // The connection may still be open, as when the worker dialed again before the hub's own timer ran out.
        link.kill();
        // Under the gate, so a worker taken up meanwhile, which holds it as it's opened, isn't taken for this one.
        let gate = self.worker_gate(workspace_id);
        let mut cleared = gate.lock().await;
        let removed = {
            let mut workers = self.workers.lock();
            let current = link.is_current(&workers, workspace_id);
            if current {
                workers.remove(workspace_id);
            }
            current
        };
        if removed {
            // What's on the instance is no longer known to be what the hub holds, so whatever clears it next, as for a
            // worker found gone, ends the worker and its agents.
            *cleared = false;
        }
        drop(cleared);
        warn!(
            name: "worker.disconnected",
            %workspace_id,
            %instance_id,
            "A worker's connection ended with no goodbye, so its workspace is disconnected until it dials again"
        );
        self.terminals_lost(link.id).await;
        self.changes_watcher_gone(workspace_id, link).await;
        self.keeper_went(Keeper::Worker(link.id)).await;
        if newly {
            let span = spans::workspace(workspace_id);
            tokio::spawn(self.clone().wait_for_worker(workspace_id.clone()).instrument(span));
        }
    }

    /// Refuses a request for a worker while the workspace's is disconnected, since starting one would end it, and it may
    /// be at work. First waits for its provider to say whether it's gone, within what a starting hub gives one, since a
    /// worker that's gone is replaced, as it was before a lost connection was waited for, and a request mustn't fail
    /// only because that hasn't been asked yet.
    pub(in super::super) async fn refuse_while_disconnected(&self, workspace_id: &WorkspaceId) -> Result<(), ApiError> {
        if !self.disconnected.contains(workspace_id) {
            return Ok(());
        }
        let _ = tokio::time::timeout(self.timers.before_serving, self.disconnected.until_settled(workspace_id)).await;
        if self.disconnected.contains(workspace_id) { Err(not_connected()) } else { Ok(()) }
    }

    /// Returns once the workspace's worker can be reached, which is at once if it can be, for what's to be sent to its
    /// agent.
    pub(crate) async fn wait_until_connected(&self, workspace_id: &WorkspaceId) {
        self.disconnected.until_connected(workspace_id).await;
    }

    /// Tells each live session of the workspace whose agent was on a connection that dropped that it's lost, as it's told
    /// when a worker is found gone as the hub runs. Which sessions did record it, so none is interrupted a second time.
    pub(in super::super) async fn tell_agents_lost(&self, workspace_id: &WorkspaceId) -> HashSet<SessionId> {
        let links = self.disconnected.dropped_links(workspace_id);
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .live
            .values()
            .filter(|session| session.workspace_id == *workspace_id)
            .cloned()
            .collect();
        let mut told = HashSet::new();
        for session in sessions {
            for link in &links {
                if session.agent_lost(*link, ExitReason::WorkerLost).await {
                    told.insert(session.id.clone());
                }
            }
        }
        told
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
        live.interrupted_with_no_agent(waiting, reason).await;
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
