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

use super::Hub;
use super::ready::ReadyWorkspace;
use crate::jobs;
use crate::session::Item;
use crate::worker_link::{Opening, STOP_GRACE, WorkerLink};

impl Hub {
    pub(super) fn worker_gate(&self, workspace_id: &WorkspaceId) -> Arc<AsyncMutex<bool>> {
        self.worker_gates.lock().entry(workspace_id.clone()).or_default().clone()
    }

    /// The running worker, if there is one, which is then kept running.
    pub(crate) fn existing_worker(&self, workspace_id: &WorkspaceId) -> Option<Arc<WorkerLink>> {
        self.open_worker(workspace_id)
    }

    /// The running worker, if there is one, without marking it wanted, for what only goes along with a worker something
    /// else started, as watching a worktree does, so a worker an earlier hub left running is still stopped if nothing
    /// else wants it.
    pub(crate) fn running_worker(&self, workspace_id: &WorkspaceId) -> Option<Arc<WorkerLink>> {
        self.workers.lock().get(workspace_id).filter(|link| link.is_open()).cloned()
    }

    /// The running worker, if there is one, kept while the map is locked, which is what makes
    /// [`Hub::stop_worker_unwanted`] safe.
    fn open_worker(&self, workspace_id: &WorkspaceId) -> Option<Arc<WorkerLink>> {
        let workers = self.workers.lock();
        let link = workers.get(workspace_id).filter(|link| link.is_open())?;
        link.keep();
        Some(link.clone())
    }

    /// The workspace's worker, starting it if needed. It's kept running.
    pub(crate) async fn ensure_worker(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
    ) -> Result<Arc<WorkerLink>, ApiError> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(ApiError::new(ErrorCode::Conflict, "the hub is shutting down"));
        }
        if let Some(found) = self.open_worker(workspace_id) {
            return Ok(found);
        }
        let workspace = self.workspace(workspace_id).await?;
        jobs::step("waiting for its turn to start the workspace's worker");
        let gate = self.worker_gate(workspace_id);
        let mut cleared = gate.lock().await;
        if let Some(found) = self.open_worker(workspace_id) {
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
        link.keep();
        self.workers.lock().insert(workspace.id.clone(), link.clone());
        // One being made has no worktree to watch yet, so it's watched once it has.
        if let Ok(ready) = ReadyWorkspace::new(workspace) {
            jobs::step(format!("asking {link} to watch the worktree's branches"));
            self.watch_branches(&ready, &link).await;
        }
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
    /// that doesn't answer, one whose version of the worker protocol this hub doesn't speak, as after two breaks or a
    /// rollback past one, or one nothing carried on with, which is no more wanted than one started for a read nobody
    /// asked for. On its own, since the hub needn't wait for that to serve, and under the workspace's gate, so a worker
    /// wanted meanwhile starts once it's done.
    pub(super) fn end_left_running(self: &Arc<Self>, workspace_id: WorkspaceId, link: Option<Arc<WorkerLink>>) {
        let hub = self.clone();
        tokio::spawn(async move { hub.let_go(&workspace_id, link, "ending a worker an earlier hub left").await });
    }

    /// Lets go of `link`, the workspace's worker, which is stopped if nothing has wanted it, as
    /// [`Hub::stop_worker_unwanted`] says, or with none, of what a start cut short may have left on the instance,
    /// which is cleared. `doing` says what for, if the workspace can't be found.
    async fn let_go(&self, workspace_id: &WorkspaceId, link: Option<Arc<WorkerLink>>, doing: &str) {
        let workspace = match self.workspace(workspace_id).await {
            Ok(workspace) => workspace,
            Err(e) => {
                return warn!(
                    name: "worker.workspace_unread",
                    %workspace_id,
                    doing,
                    error = e.message,
                    "Couldn't read a workspace to let go of its worker"
                );
            }
        };
        match link {
            Some(link) => self.stop_worker_unwanted(&workspace, &link).await,
            None => self.clear_unclaimed_instance(&workspace).await,
        }
    }

    /// Clears the instance of whatever may be left there, unless a worker in the map holds it, or nothing can be, as
    /// [`Hub::worker_gates`] says.
    async fn clear_unclaimed_instance(&self, workspace: &Workspace) {
        let provider = match self.provider(&workspace.provider) {
            Ok(provider) => provider,
            Err(e) => {
                return warn!(
                    name: "instance.uncleared",
                    workspace_id = %workspace.id,
                    error = e.message,
                    "Couldn't clear an instance"
                );
            }
        };
        jobs::step("waiting for its turn to clear the workspace's instance");
        let gate = self.worker_gate(&workspace.id);
        let mut cleared = gate.lock().await;
        if !*cleared && self.workers.lock().get(&workspace.id).is_none() {
            jobs::step("clearing the workspace's instance");
            *cleared = clear_instance(provider.as_ref(), &workspace.instance_id).await;
        }
    }

    /// Stops a worker nothing has wanted since an earlier hub left it running. Under its gate, so a worker wanted
    /// meanwhile starts once this one has gone, rather than finding its instance still taken.
    pub(super) async fn stop_worker_unwanted(&self, workspace: &Workspace, link: &Arc<WorkerLink>) {
        jobs::step(format!("waiting for its turn to stop {link}"));
        let gate = self.worker_gate(&workspace.id);
        let mut cleared = gate.lock().await;
        {
            let mut workers = self.workers.lock();
            let unwanted = link.is_current(&workers, &workspace.id) && !link.kept();
            if !unwanted {
                return;
            }
            workers.remove(&workspace.id);
        }
        self.stop_and_clear(workspace, link, &mut cleared, "stopping a worker an earlier hub left").await;
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
                warn!(
                    name: "instance.uncleared",
                    workspace_id = %workspace.id,
                    doing,
                    error = e.message,
                    "Couldn't clear an instance"
                );
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
        // The instance is free, so a replacement can start or stop from here on. What's left waits on other workers,
        // such as a replacement told to watch, which mustn't keep the gate from the next.
        drop(cleared);
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
        // And what it watched.
        self.changes_watcher_gone(workspace_id, link).await;
        if current {
            self.branches_let_go(workspace_id).await;
        }
    }

    /// Asks a worker to stop and waits for its output to end. If it's stuck, it's ended as [`Hub::end_stuck_worker`]
    /// says.
    pub(super) async fn close_worker(&self, workspace: &Workspace, link: &WorkerLink, provider: &dyn Provider) {
        link.close().await;
        if tokio::time::timeout(self.timers.shutdown_grace, link.wait_closed()).await.is_ok() {
            return;
        }
        warn!(name: "worker.not_stopped", instance_id = %workspace.instance_id, "A worker didn't stop in time");
        self.end_stuck_worker(workspace, link, provider).await;
    }

    /// The same as [`Hub::end_stuck_worker`], for the worker of the workspace `id`, as the hub stops.
    pub(super) async fn end_stuck_worker_of(&self, id: &WorkspaceId, link: &WorkerLink) {
        match self.workspace(id).await.and_then(|workspace| Ok((self.provider(&workspace.provider)?, workspace))) {
            Ok((provider, workspace)) => self.end_stuck_worker(&workspace, link, provider.as_ref()).await,
            Err(e) => {
                warn!(
                    name: "worker.stuck_unended",
                    workspace_id = %id,
                    error = e.message,
                    "Couldn't end a stuck worker without its workspace"
                );
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
        if !replaced && let Err(e) = provider.kill_worker(instance_id).await {
            warn!(name: "worker.stuck_unended", %instance_id, error = error_chain(&e), "Couldn't end a stuck worker");
            *cleared = false;
        }
    }
}

/// Ends whatever worker runs on the instance, so another can start there and claim it. Whether that worked, since until it
/// has, a new worker may find the instance taken.
async fn clear_instance(provider: &dyn Provider, instance_id: &InstanceId) -> bool {
    match provider.end_worker(instance_id).await {
        Ok(()) => true,
        Err(e) => {
            warn!(
                name: "worker.unended",
                %instance_id,
                error = error_chain(&e),
                "Couldn't end the worker on an instance"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use protocol::worker::WorkerResponse;

    use super::*;
    use crate::hub::tests::{hub_with_worktree, serving};

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
        let stopping = tokio::time::timeout(Duration::from_secs(1), hub.stop_worker_unwanted(&workspace, &link)).await;

        assert!(stopping.is_err(), "the stop was still waiting for the worker to exit");
        assert!(hub.workers.lock().get(&id).is_none(), "it had let go of the worker");
        assert!(!*hub.worker_gate(&id).lock().await, "the worker may still be on the instance");
    }
}
