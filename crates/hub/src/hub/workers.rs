//! Starting and stopping each workspace's worker, one at a time per workspace.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::event::ExitReason;
use protocol::model::{InstanceId, Workspace, WorkspaceId};
use protocol::worker::RunningAgent;
use providers::Provider;
use tokio::sync::Mutex as AsyncMutex;
use tracing::warn;

use super::{Hub, SHUTDOWN_GRACE, Wanted};
use crate::session::Item;
use crate::worker_link::{Opening, STOP_GRACE, WorkerLink};

impl Hub {
    pub(super) fn worker_gate(&self, workspace_id: &WorkspaceId) -> Arc<AsyncMutex<bool>> {
        self.worker_gates.lock().entry(workspace_id.clone()).or_default().clone()
    }

    /// The running worker, if there is one, which is then kept running.
    pub(crate) fn existing_worker(&self, workspace_id: &WorkspaceId) -> Option<Arc<WorkerLink>> {
        self.open_worker(workspace_id, Wanted::ForAnything)
    }

    /// The running worker, if there is one. One wanted for anything is marked kept while the map is locked, which is
    /// what makes [`Hub::stop_worker_only_read`] safe. One being replaced is handed to nothing, so whatever wants it
    /// waits for the workspace's gate, and the worker that replaces it.
    fn open_worker(&self, workspace_id: &WorkspaceId, wanted: Wanted) -> Option<Arc<WorkerLink>> {
        let workers = self.workers.lock();
        let link = workers.get(workspace_id).filter(|link| link.is_open() && !link.replacing())?;
        if wanted == Wanted::ForAnything {
            link.keep();
        }
        Some(link.clone())
    }

    /// The workspace's worker, starting it if needed. It's kept running.
    pub(crate) async fn ensure_worker(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
    ) -> Result<Arc<WorkerLink>, ApiError> {
        self.worker(workspace_id, Wanted::ForAnything).await
    }

    /// The same, for a read nobody asked for, after which [`Hub::stop_worker_only_read`] stops it again.
    pub(super) async fn worker_to_read(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
    ) -> Result<Arc<WorkerLink>, ApiError> {
        self.worker(workspace_id, Wanted::OnlyToRead).await
    }

    async fn worker(self: &Arc<Self>, workspace_id: &WorkspaceId, wanted: Wanted) -> Result<Arc<WorkerLink>, ApiError> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(ApiError::new(ErrorCode::Conflict, "the hub is shutting down"));
        }
        if let Some(link) = self.open_worker(workspace_id, wanted) {
            return Ok(link);
        }
        let workspace = self.workspace(workspace_id).await?;
        let gate = self.worker_gate(workspace_id);
        let mut cleared = gate.lock().await;
        if let Some(link) = self.open_worker(workspace_id, wanted) {
            return Ok(link);
        }
        let provider = self.provider(&workspace.provider)?;
        // One whose connection has ended, but whose cleanup is waiting for this gate, may still hold the instance.
        let ended = self.workers.lock().get(workspace_id).cloned();
        if let Some(ended) = &ended {
            ended.stop(STOP_GRACE).await;
        }
        // Or what an earlier hub left that this one didn't carry on with, such as a worker that was stopping, or the
        // agents of one that was killed. Or a stop that failed.
        if ended.is_some() || !*cleared {
            *cleared = clear_instance(provider.as_ref(), &workspace.instance_id).await;
        }
        // Whichever provider runs it, what its worker writes to stderr is kept here.
        let log = self.worker_logs_dir.join(format!("{}.log", workspace.instance_id));
        let (id, instance_id) = (workspace.id.clone(), workspace.instance_id.clone());
        let link = WorkerLink::open(self.clone(), provider, id, instance_id, &log, Opening::Started).await?;
        if wanted == Wanted::ForAnything {
            link.keep();
        }
        self.workers.lock().insert(workspace.id.clone(), link.clone());
        self.watch_branches(&workspace, &link).await;
        Ok(link)
    }

    /// The worker an earlier hub left running on the workspace's instance, as the hub starts, and what it's running.
    /// What it sends is held until [`WorkerLink::release`], so the hub can say first which agent each of its sessions
    /// has. Not kept, until a session carries on with it. One that can't be reached, or doesn't say what it's running,
    /// is ended, as [`Hub::end_left_running`] says.
    pub(crate) async fn worker_left_running(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
    ) -> Result<(Arc<WorkerLink>, Vec<RunningAgent>), ApiError> {
        let workspace = self.workspace(workspace_id).await?;
        let provider = self.provider(&workspace.provider)?;
        let gate = self.worker_gate(workspace_id);
        let mut cleared = gate.lock().await;
        let log = self.worker_logs_dir.join(format!("{}.log", workspace.instance_id));
        let (id, instance_id) = (workspace.id.clone(), workspace.instance_id);
        let link = match WorkerLink::open(self.clone(), provider, id, instance_id, &log, Opening::LeftRunning).await {
            Ok(link) => link,
            Err(e) => {
                drop(cleared);
                self.end_left_running(workspace.id, None);
                return Err(e);
            }
        };
        // It holds the instance, so nothing else is running there.
        *cleared = true;
        self.workers.lock().insert(workspace.id.clone(), link.clone());
        drop(cleared);
        match link.running_agents().await {
            Ok(running) => Ok((link, running)),
            Err(e) => {
                link.release(self).await;
                self.end_left_running(workspace.id, Some(link));
                Err(e)
            }
        }
    }

    /// Ends what an earlier hub left on the workspace's instance that this hub doesn't carry on with: its worker, with
    /// `link` if it was reached, and the agents with it, so none goes on unseen. Such as a worker that's stopping, one
    /// that doesn't answer, or one nothing carried on with, which is no more wanted than one started for a read nobody
    /// asked for. On its own, since the hub needn't wait for that to serve, and under the workspace's gate, so a worker
    /// wanted meanwhile starts once it's done.
    pub(super) fn end_left_running(self: &Arc<Self>, workspace_id: WorkspaceId, link: Option<Arc<WorkerLink>>) {
        let hub = self.clone();
        tokio::spawn(async move {
            let ended = hub.workspace(&workspace_id).await.and_then(|workspace| {
                let provider = hub.provider(&workspace.provider)?;
                Ok((workspace, provider))
            });
            let (workspace, provider) = match ended {
                Ok(ended) => ended,
                Err(e) => return warn!(%workspace_id, "ending a worker an earlier hub left: {}", e.message),
            };
            if let Some(link) = link {
                return hub.stop_worker_only_read(&workspace, &link).await;
            }
            let gate = hub.worker_gate(&workspace.id);
            let mut cleared = gate.lock().await;
            if !*cleared {
                *cleared = clear_instance(provider.as_ref(), &workspace.instance_id).await;
            }
        });
    }

    /// Stops a worker that only a read nobody asked for has wanted, so a hub with many workspaces doesn't keep one
    /// running for each. Under its gate, so a worker wanted meanwhile starts once this one has gone, rather than
    /// finding its instance still taken.
    pub(super) async fn stop_worker_only_read(&self, workspace: &Workspace, link: &Arc<WorkerLink>) {
        let gate = self.worker_gate(&workspace.id);
        let mut cleared = gate.lock().await;
        {
            let mut workers = self.workers.lock();
            let only_read = link.is_current(&workers, &workspace.id) && !link.kept();
            if !only_read {
                return;
            }
            workers.remove(&workspace.id);
        }
        self.stop_and_clear(workspace, link, &mut cleared, "stopping a worker started to read a pull request").await;
    }

    /// Stops `link`, which is out of the hub's map of workers by now, and gives back its instance. Under the
    /// workspace's gate, whose `cleared` this sets. `doing` says what stopping it was for, if giving it back fails.
    pub(super) async fn stop_and_clear(
        &self,
        workspace: &Workspace,
        link: &WorkerLink,
        cleared: &mut bool,
        doing: &str,
    ) {
        link.stop(STOP_GRACE).await;
        *cleared = match self.provider(&workspace.provider) {
            Ok(provider) => clear_instance(provider.as_ref(), &workspace.instance_id).await,
            Err(e) => {
                warn!(id = %workspace.id, "{doing}: {}", e.message);
                false
            }
        };
    }

    /// Called when a worker's stream ends, after everything it sent has been queued.
    pub(crate) async fn worker_gone(
        &self,
        workspace_id: &WorkspaceId,
        link: &Arc<WorkerLink>,
        provider: &dyn Provider,
        instance_id: &InstanceId,
    ) {
        let gate = self.worker_gate(workspace_id);
        let mut cleared = gate.lock().await;
        // Under the gate, so a replacement can't start while this one still holds the instance.
        link.stop(STOP_GRACE).await;
        let current = {
            let mut workers = self.workers.lock();
            let current = link.is_current(&workers, workspace_id);
            if current {
                workers.remove(workspace_id);
            }
            current
        };
        // Only the hub's current worker has the instance to give back. One stopped or replaced already doesn't, and
        // nor does one that never got that far, such as one refused because another worker serves the instance.
        if current {
            *cleared = clear_instance(provider, instance_id).await;
        }
        // A replacement may have started already, once this one ended. A session tells them apart by the link's ID.
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .live
            .values()
            .filter(|session| &session.workspace_id == workspace_id)
            .cloned()
            .collect();
        for session in sessions {
            session.send_item(Item::AgentLost { link: link.id, reason: ExitReason::WorkerLost }).await;
        }
    }

    /// Asks a worker to stop and waits for its output to end. If it's stuck, it's ended as [`Hub::end_stuck_worker`]
    /// says.
    pub(super) async fn close_worker(&self, workspace: &Workspace, link: &WorkerLink, provider: &dyn Provider) {
        link.close().await;
        if tokio::time::timeout(SHUTDOWN_GRACE, link.wait_closed()).await.is_ok() {
            return;
        }
        warn!(instance_id = %workspace.instance_id, "a worker didn't stop in time");
        self.end_stuck_worker(workspace, link, provider).await;
    }

    /// The same as [`Hub::end_stuck_worker`], for the worker of the workspace `id`, as the hub stops.
    pub(super) async fn end_stuck_worker_of(&self, id: &WorkspaceId, link: &WorkerLink) {
        match self.workspace(id).await.and_then(|workspace| Ok((self.provider(&workspace.provider)?, workspace))) {
            Ok((provider, workspace)) => self.end_stuck_worker(&workspace, link, provider.as_ref()).await,
            Err(e) => {
                warn!(%id, "ending a stuck worker without its workspace: {}", e.message);
                link.kill();
            }
        }
    }

    /// Ends a worker that didn't stop when asked, and has its provider end what it left running, such as its agents.
    /// Under the gate, so that can't end a worker started meanwhile.
    async fn end_stuck_worker(&self, workspace: &Workspace, link: &WorkerLink, provider: &dyn Provider) {
        let instance_id = &workspace.instance_id;
        let gate = self.worker_gate(&workspace.id);
        let mut cleared = gate.lock().await;
        link.kill();
        let replaced = self.workers.lock().get(&workspace.id).is_some_and(|current| current.is_open());
        if !replaced && let Err(e) = provider.kill(instance_id).await {
            warn!(%instance_id, "stopping a stuck worker failed: {}", error_chain(&e));
            *cleared = false;
        }
    }
}

/// Stops whatever runs on the instance, so a worker can start there and claim it. Whether that worked, since until it
/// has, a new worker may find the instance taken.
async fn clear_instance(provider: &dyn Provider, instance_id: &InstanceId) -> bool {
    match provider.stop(instance_id).await {
        Ok(()) => true,
        Err(e) => {
            warn!(%instance_id, "stopping an instance failed: {}", error_chain(&e));
            false
        }
    }
}
