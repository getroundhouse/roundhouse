//! Starting and stopping each workspace's worker, one at a time per workspace.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::{InstanceId, Workspace, WorkspaceId};
use providers::Provider;
use tokio::sync::Mutex as AsyncMutex;
use tracing::warn;

use super::{Hub, SHUTDOWN_GRACE, Wanted};
use crate::session::Item;
use crate::worker_link::WorkerLink;
use crate::worker_process::STOP_GRACE;

impl Hub {
    fn worker_gate(&self, workspace_id: &WorkspaceId) -> Arc<AsyncMutex<bool>> {
        self.worker_gates.lock().entry(workspace_id.clone()).or_default().clone()
    }

    /// The running worker, if there is one, which is then kept running.
    pub(crate) fn existing_worker(&self, workspace_id: &WorkspaceId) -> Option<Arc<WorkerLink>> {
        self.open_worker(workspace_id, Wanted::ForAnything)
    }

    /// The running worker, if there is one. One wanted for anything is marked kept while the map is locked, which is
    /// what makes [`Hub::stop_worker_only_read`] safe.
    fn open_worker(&self, workspace_id: &WorkspaceId, wanted: Wanted) -> Option<Arc<WorkerLink>> {
        let workers = self.workers.lock();
        let link = workers.get(workspace_id).filter(|link| link.is_open())?;
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
        // As may a worker that an earlier hub left. It stops once its stdin closes, but may not have yet.
        if ended.is_some() || !*cleared {
            *cleared = clear_instance(provider.as_ref(), &workspace.instance_id).await;
        }
        // Whichever provider runs it, what its worker writes to stderr is kept here.
        let log = self.worker_logs_dir.join(format!("{}.log", workspace.instance_id));
        let link = WorkerLink::open(self.clone(), provider, workspace.id.clone(), workspace.instance_id, &log).await?;
        if wanted == Wanted::ForAnything {
            link.keep();
        }
        self.workers.lock().insert(workspace.id, link.clone());
        Ok(link)
    }

    /// Stops a worker that only a read nobody asked for has wanted, so a hub with many workspaces doesn't keep one
    /// running for each. Under its gate, so a worker wanted meanwhile starts once this one has gone, rather than
    /// finding its instance still taken.
    pub(super) async fn stop_worker_only_read(&self, workspace: &Workspace, link: &Arc<WorkerLink>) {
        let gate = self.worker_gate(&workspace.id);
        let mut cleared = gate.lock().await;
        {
            let mut workers = self.workers.lock();
            let only_read =
                workers.get(&workspace.id).is_some_and(|current| Arc::ptr_eq(current, link) && !link.kept());
            if !only_read {
                return;
            }
            workers.remove(&workspace.id);
        }
        link.stop(STOP_GRACE).await;
        *cleared = match self.provider(&workspace.provider) {
            Ok(provider) => clear_instance(provider.as_ref(), &workspace.instance_id).await,
            Err(e) => {
                warn!(id = %workspace.id, "stopping a worker started to read a pull request: {}", e.message);
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
            let current = workers.get(workspace_id).is_some_and(|current| Arc::ptr_eq(current, link));
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
            session.send_item(Item::WorkerLost(link.id)).await;
        }
    }

    /// Asks a worker to stop and waits for its output to end. If it's stuck, its provider ends it, under the gate, so
    /// that can't end a worker started meanwhile.
    pub(super) async fn close_worker(&self, workspace: &Workspace, link: &WorkerLink, provider: &dyn Provider) {
        link.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, link.wait_closed()).await.is_ok() {
            return;
        }
        let instance_id = &workspace.instance_id;
        warn!(%instance_id, "a worker didn't stop in time");
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
