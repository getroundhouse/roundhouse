//! Running a workspace's setup script.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{Repo, SetupStatus, Workspace, WorkspaceId};
use protocol::worker::{WorkerRequest, WorkerResponse};
use tokio::sync::watch;
use tracing::warn;

use super::workspaces::workspace_env;
use super::{Hub, unexpected};

/// A workspace's setup, from when the hub records it as running until it records how it ended.
pub(super) struct SetupGate {
    /// Prompts wait on this.
    done: watch::Sender<bool>,
    /// Asks for the script to be stopped, whether or not it has started yet.
    stop: watch::Sender<bool>,
}

#[cfg(test)]
impl SetupGate {
    /// One for a script that's running, as far as anything that only reads the hub's map of them can tell.
    pub(super) fn running() -> Self {
        Self { done: watch::channel(false).0, stop: watch::channel(false).0 }
    }
}

impl Hub {
    /// Records that setup is running, starts it in the background, and returns the workspace as stored,
    /// so a reply says only what the hub has recorded. Prompts wait for setup.
    pub(super) async fn start_setup(self: &Arc<Self>, repo: Repo, workspace: Workspace) -> Result<Workspace, ApiError> {
        let (done, _) = watch::channel(false);
        let (stop, stopped) = watch::channel(false);
        self.setup_gates.lock().insert(workspace.id.clone(), SetupGate { done: done.clone(), stop });
        self.work_changed();
        let marked = workspace.id.clone();
        let running = self.blocking(move |store| store.set_workspace_setup(&marked, SetupStatus::Running)).await;
        let running = match running {
            Ok(running) => running,
            Err(e) => {
                self.setup_gates.lock().remove(&workspace.id);
                self.work_changed();
                done.send_replace(true);
                return Err(e);
            }
        };
        self.notify(HubNotice::Workspace(running.clone()));
        let hub = self.clone();
        let id = workspace.id.clone();
        tokio::spawn(async move {
            let outcome = hub.run_setup(&repo, &workspace, stopped).await;
            let marked = id.clone();
            match hub.blocking(move |store| store.set_workspace_setup(&marked, outcome)).await {
                Ok(workspace) => hub.notify(HubNotice::Workspace(workspace)),
                Err(e) => warn!(workspace = %id, "recording setup: {e}"),
            }
            hub.setup_gates.lock().remove(&id);
            hub.work_changed();
            done.send_replace(true);
        });
        Ok(running)
    }

    /// Asks the worker to stop the script once `stopped` says so. Only once the request to run it is on
    /// its way, so the worker always knows which script is meant, even if it hasn't started it yet.
    async fn run_setup(
        self: &Arc<Self>,
        repo: &Repo,
        workspace: &Workspace,
        mut stopped: watch::Receiver<bool>,
    ) -> SetupStatus {
        let Some(path) = workspace.path.clone() else { return SetupStatus::Failed { code: None } };
        let link = match self.ensure_worker(&workspace.id).await {
            Ok(link) => link,
            Err(e) => {
                warn!(workspace = workspace.name, "setup couldn't start: {}", e.message);
                return SetupStatus::Failed { code: None };
            }
        };
        let request =
            WorkerRequest::SetupWorkspace { repo_path: repo.path.clone(), path, env: workspace_env(workspace, repo) };
        let finished = async {
            let running = link.send(request).await?;
            tokio::pin!(running);
            tokio::select! {
                reply = &mut running => reply,
                Ok(()) = async { stopped.wait_for(|stop| *stop).await.map(drop) } => {
                    let _ = link.call(WorkerRequest::CancelSetup).await;
                    running.await
                }
            }
        };
        match finished.await {
            Ok(WorkerResponse::SetupDone { code: Some(0) }) => SetupStatus::Succeeded,
            Ok(WorkerResponse::SetupDone { code }) => SetupStatus::Failed { code },
            Ok(other) => {
                warn!(workspace = workspace.name, "unexpected reply to setup: {other:?}");
                SetupStatus::Failed { code: None }
            }
            Err(e) => {
                warn!(workspace = workspace.name, "setup failed: {}", e.message);
                SetupStatus::Failed { code: None }
            }
        }
    }

    /// Waits for setup, if it's running. Agents shouldn't start in a half-prepared worktree.
    pub(crate) async fn wait_for_setup(&self, workspace_id: &WorkspaceId) {
        let gate = self.setup_gates.lock().get(workspace_id).map(|gate| gate.done.subscribe());
        if let Some(mut gate) = gate {
            let _ = gate.wait_for(|done| *done).await;
        }
    }

    pub(crate) async fn workspace_setup_log(self: &Arc<Self>, id: WorkspaceId) -> Result<String, ApiError> {
        let (_, path, link) = self.ready_worktree(&id).await?;
        let reply = link.call(WorkerRequest::SetupLog { path }).await?;
        let WorkerResponse::SetupLog { text } = reply else {
            return Err(unexpected(&reply));
        };
        Ok(text)
    }

    pub(crate) async fn workspace_rerun_setup(self: &Arc<Self>, id: WorkspaceId) -> Result<Workspace, ApiError> {
        let (workspace, _, _) = self.ready_worktree(&id).await?;
        if self.setup_gates.lock().contains_key(&id) {
            return Err(ApiError::new(ErrorCode::Conflict, "setup is already running"));
        }
        let repo_id = workspace.repo_id.clone();
        let repo = self.blocking(move |store| store.get_repo(&repo_id)).await?;
        self.start_setup(repo, workspace).await
    }

    /// Stops the setup script if it's running, or keeps it from starting if it hasn't yet. The workspace stays
    /// usable.
    pub(crate) fn cancel_setup(&self, workspace_id: &WorkspaceId) {
        if let Some(gate) = self.setup_gates.lock().get(workspace_id) {
            gate.stop.send_replace(true);
        }
    }
}
