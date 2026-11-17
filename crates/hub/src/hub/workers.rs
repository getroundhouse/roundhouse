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

use super::ready::ReadyWorkspace;
use super::{Hub, Wanted};
use crate::jobs;
use crate::session::Item;
use crate::worker_link::{Opening, ReadHold, STOP_GRACE, WorkerLink};

impl Hub {
    pub(super) fn worker_gate(&self, workspace_id: &WorkspaceId) -> Arc<AsyncMutex<bool>> {
        self.worker_gates.lock().entry(workspace_id.clone()).or_default().clone()
    }

    /// The running worker, if there is one, which is then kept running.
    pub(crate) fn existing_worker(&self, workspace_id: &WorkspaceId) -> Option<Arc<WorkerLink>> {
        self.open_worker(workspace_id, Wanted::ForAnything).map(|(link, _)| link)
    }

    /// The running worker, if there is one, as [`wanted_for`] marks it while the map is locked, which is what makes
    /// [`Hub::stop_worker_only_read`] safe.
    fn open_worker(&self, workspace_id: &WorkspaceId, wanted: Wanted) -> Option<(Arc<WorkerLink>, Option<ReadHold>)> {
        let workers = self.workers.lock();
        let link = workers.get(workspace_id).filter(|link| link.is_open())?;
        Some((link.clone(), wanted_for(link, wanted)))
    }

    /// The workspace's worker, starting it if needed. It's kept running.
    pub(crate) async fn ensure_worker(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
    ) -> Result<Arc<WorkerLink>, ApiError> {
        Ok(self.worker(workspace_id, Wanted::ForAnything).await?.0)
    }

    /// The same, for a read nobody asked for, held for it until the hold goes, after which
    /// [`Hub::stop_worker_only_read`] stops it again, unless another read holds it.
    pub(super) async fn worker_to_read(self: &Arc<Self>, workspace_id: &WorkspaceId) -> Result<ReadHold, ApiError> {
        jobs::step("getting the workspace's worker");
        let (link, held) = self.worker(workspace_id, Wanted::OnlyToRead).await?;
        Ok(held.unwrap_or_else(|| link.hold_for_read()))
    }

    async fn worker(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
        wanted: Wanted,
    ) -> Result<(Arc<WorkerLink>, Option<ReadHold>), ApiError> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(ApiError::new(ErrorCode::Conflict, "the hub is shutting down"));
        }
        if let Some(found) = self.open_worker(workspace_id, wanted) {
            return Ok(found);
        }
        let workspace = self.workspace(workspace_id).await?;
        jobs::step("waiting for its turn to start the workspace's worker");
        let gate = self.worker_gate(workspace_id);
        let mut cleared = gate.lock().await;
        if let Some(found) = self.open_worker(workspace_id, wanted) {
            return Ok(found);
        }
        let provider = self.provider(&workspace.provider)?;
        // One whose connection has ended, but whose cleanup is waiting for this gate, may still hold the instance.
        let ended = self.workers.lock().get(workspace_id).cloned();
        if let Some(ended) = &ended {
            jobs::step(format!("stopping {ended}, whose connection ended, before starting another"));
            ended.stop(STOP_GRACE).await;
        }
        // Or what an earlier hub left that this one didn't carry on with, such as a worker that was stopping, or the
        // agents of one that was killed. Or a stop that failed.
        if ended.is_some() || !*cleared {
            jobs::step("clearing the workspace's instance before starting its worker");
            *cleared = clear_instance(provider.as_ref(), &workspace.instance_id).await;
        }
        jobs::step("starting the workspace's worker");
        // Whichever provider runs it, what its worker writes to stderr is kept here.
        let log = self.worker_logs_dir.join(format!("{}.log", workspace.instance_id));
        let (id, instance_id) = (workspace.id.clone(), workspace.instance_id.clone());
        // Its worker is on the instance from the moment it starts, and not in the map until it's opened, so a start
        // dropped between leaves the next one to clear the instance first, and so does one that failed, which may not
        // have ended what it started. One refused because another worker serves the instance leaves it as it was,
        // since clearing it would end that worker.
        let was_cleared = std::mem::replace(&mut *cleared, false);
        let opened = WorkerLink::open(self.clone(), provider, id, instance_id, &log, Opening::Started).await;
        let refused = opened.as_ref().is_err_and(|e| e.code == ErrorCode::Conflict);
        if opened.is_ok() || refused {
            *cleared = was_cleared;
        }
        let link = opened?;
        let held = wanted_for(&link, wanted);
        self.workers.lock().insert(workspace.id.clone(), link.clone());
        // One being made has no worktree to watch yet, so it's watched once it has.
        if let Ok(ready) = ReadyWorkspace::new(workspace) {
            jobs::step(format!("asking {link} to watch the worktree's branches"));
            self.watch_branches(&ready, &link).await;
        }
        Ok((link, held))
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
        tokio::spawn(async move { hub.let_go(&workspace_id, link, "ending a worker an earlier hub left").await });
    }

    /// What a read nobody asked for leaves once it has ended, however it ended, let go of, so a read stopped partway
    /// leaves no worker running unseen.
    pub(super) async fn let_go_after_read(&self, workspace_id: &WorkspaceId) {
        let current = self.workers.lock().get(workspace_id).cloned();
        self.let_go(workspace_id, current, "letting go of a worker after a read").await;
    }

    /// Lets go of `link`, the workspace's worker, which is stopped if only reads wanted it, as
    /// [`Hub::stop_worker_only_read`] says, or with none, of what a start cut short may have left on the instance,
    /// which is cleared. `doing` says what for, if the workspace can't be found.
    async fn let_go(&self, workspace_id: &WorkspaceId, link: Option<Arc<WorkerLink>>, doing: &str) {
        let workspace = match self.workspace(workspace_id).await {
            Ok(workspace) => workspace,
            Err(e) => return warn!(%workspace_id, "{doing}: {}", e.message),
        };
        match link {
            Some(link) => self.stop_worker_only_read(&workspace, &link).await,
            None => self.clear_unclaimed_instance(&workspace).await,
        }
    }

    /// Clears the instance of whatever may be left there, unless a worker in the map holds it, or nothing can be, as
    /// [`Hub::worker_gates`] says.
    async fn clear_unclaimed_instance(&self, workspace: &Workspace) {
        let provider = match self.provider(&workspace.provider) {
            Ok(provider) => provider,
            Err(e) => return warn!(id = %workspace.id, "clearing an instance: {}", e.message),
        };
        jobs::step("waiting for its turn to clear the workspace's instance");
        let gate = self.worker_gate(&workspace.id);
        let mut cleared = gate.lock().await;
        if !*cleared && self.workers.lock().get(&workspace.id).is_none() {
            jobs::step("clearing the workspace's instance");
            *cleared = clear_instance(provider.as_ref(), &workspace.instance_id).await;
        }
    }

    /// Whether the workspace has a worker running, without keeping it for anything, as a read nobody asked for asks.
    pub(super) fn has_worker(&self, workspace_id: &WorkspaceId) -> bool {
        self.workers.lock().get(workspace_id).is_some_and(|link| link.is_open())
    }

    /// Stops a worker that only reads nobody asked for have wanted, once none of them holds it, so a hub with many
    /// workspaces doesn't keep one running for each. Under its gate, so a worker wanted meanwhile starts once this one has gone, rather than
    /// finding its instance still taken.
    pub(super) async fn stop_worker_only_read(&self, workspace: &Workspace, link: &Arc<WorkerLink>) {
        jobs::step(format!("waiting for its turn to stop {link}"));
        let gate = self.worker_gate(&workspace.id);
        let mut cleared = gate.lock().await;
        {
            let mut workers = self.workers.lock();
            let only_read = link.is_current(&workers, &workspace.id) && !link.kept() && !link.held_for_read();
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
        // It's out of the map, and on the instance until it has stopped, so a stop dropped partway leaves the next
        // worker to clear the instance first.
        *cleared = false;
        jobs::step(format!("stopping {link}"));
        link.stop(STOP_GRACE).await;
        jobs::step(format!("clearing the workspace's instance after stopping {link}"));
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
                // As for a stop: out of the map, the instance isn't clear until it has been cleared.
                *cleared = false;
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
        // Their shells have gone with it.
        self.terminals_lost(link.id).await;
    }

    /// Asks a worker to stop and waits for its output to end. If it's stuck, it's ended as [`Hub::end_stuck_worker`]
    /// says.
    pub(super) async fn close_worker(&self, workspace: &Workspace, link: &WorkerLink, provider: &dyn Provider) {
        link.close().await;
        if tokio::time::timeout(self.timers.shutdown_grace, link.wait_closed()).await.is_ok() {
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

/// Marks `link` as `wanted` says: kept, for anything, or held for a read nobody asked for, by the hold this gives. Only
/// before it's in the hub's map of workers, or while that's locked, as [`WorkerLink::keep`] says.
fn wanted_for(link: &Arc<WorkerLink>, wanted: Wanted) -> Option<ReadHold> {
    match wanted {
        Wanted::ForAnything => {
            link.keep();
            None
        }
        Wanted::OnlyToRead => Some(link.hold_for_read()),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use protocol::worker::WorkerResponse;

    use super::*;
    use crate::hub::tests::{hub_with_worktree, serving};

    /// As a wait read that took the worker for its machine's token ends while the workspace's own read still uses it.
    #[tokio::test(start_paused = true)]
    async fn a_read_that_ends_stops_a_worker_only_it_held_and_not_one_another_read_holds() {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_with_worktree(dir.path());
        let link = WorkerLink::answering_until_killed(|_| Some(Ok(WorkerResponse::Ok)));
        serving(&hub, &link);
        let id: WorkspaceId = "w".into();
        let (one, another) = (link.hold_for_read(), link.hold_for_read());

        drop(one);
        hub.let_go_after_read(&id).await;
        let kept_for_another = hub.workers.lock().get(&id).is_some();
        drop(another);
        hub.let_go_after_read(&id).await;

        assert!(kept_for_another, "another read held it");
        assert!(hub.workers.lock().get(&id).is_none(), "once none held it, it was stopped");
    }

    #[tokio::test(start_paused = true)]
    async fn a_stop_dropped_partway_leaves_the_next_worker_to_clear_the_instance() {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_with_worktree(dir.path());
        let link = WorkerLink::answering_until_killed(|_| Some(Ok(WorkerResponse::Ok)));
        serving(&hub, &link);
        let id: WorkspaceId = "w".into();
        // As once a worker of its own holds the instance.
        *hub.worker_gate(&id).lock().await = true;
        let workspace = hub.workspace(&id).await.unwrap();

        // Dropped while it waits for the worker to stop, as by a caller that gives up on it.
        let stopping = tokio::time::timeout(Duration::from_secs(1), hub.stop_worker_only_read(&workspace, &link)).await;

        assert!(stopping.is_err(), "the stop was still waiting for the worker to exit");
        assert!(hub.workers.lock().get(&id).is_none(), "it had let go of the worker");
        assert!(!*hub.worker_gate(&id).lock().await, "the worker may still be on the instance");
    }
}
