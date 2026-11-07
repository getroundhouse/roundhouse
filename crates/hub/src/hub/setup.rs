//! Running a workspace's setup script.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{Repo, SetupStatus, Workspace};
use protocol::worker::{WorkerRequest, WorkerResponse};
use tokio::sync::watch;
use tracing::warn;

use super::workspaces::workspace_env;
use super::{Hub, unexpected};

impl Hub {
    /// Starts setup in the background, and returns the workspace as it now stands. Prompts wait for it.
    pub(super) fn start_setup(self: &Arc<Self>, repo: Repo, workspace: Workspace) -> Workspace {
        let (gate_tx, _) = watch::channel(false);
        self.setup_gates.lock().insert(workspace.id.clone(), gate_tx.clone());
        let mut running = workspace.clone();
        running.setup = SetupStatus::Running;
        let hub = self.clone();
        let id = workspace.id.clone();
        tokio::spawn(async move {
            let marked = id.clone();
            match hub.blocking(move |store| store.set_workspace_setup(&marked, SetupStatus::Running)).await {
                Ok(workspace) => hub.notify(HubNotice::Workspace(workspace)),
                Err(e) => warn!(workspace = id, "recording setup: {e}"),
            }
            let outcome = hub.run_setup(&repo, &workspace).await;
            let marked = id.clone();
            match hub.blocking(move |store| store.set_workspace_setup(&marked, outcome)).await {
                Ok(workspace) => hub.notify(HubNotice::Workspace(workspace)),
                Err(e) => warn!(workspace = id, "recording setup: {e}"),
            }
            hub.setup_gates.lock().remove(&id);
            gate_tx.send_replace(true);
        });
        running
    }

    async fn run_setup(self: &Arc<Self>, repo: &Repo, workspace: &Workspace) -> SetupStatus {
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
        match link.call(request).await {
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
    pub(crate) async fn wait_for_setup(&self, workspace_id: &str) {
        let gate = self.setup_gates.lock().get(workspace_id).map(tokio::sync::watch::Sender::subscribe);
        if let Some(mut gate) = gate {
            let _ = gate.wait_for(|done| *done).await;
        }
    }

    pub(crate) async fn workspace_setup_log(self: &Arc<Self>, id: String) -> Result<String, ApiError> {
        let (_, path, link) = self.ready_worktree(&id).await?;
        let reply = link.call(WorkerRequest::SetupLog { path }).await?;
        let WorkerResponse::SetupLog { text } = reply else {
            return Err(unexpected(&reply));
        };
        Ok(text)
    }

    pub(crate) async fn workspace_rerun_setup(self: &Arc<Self>, id: String) -> Result<Workspace, ApiError> {
        let (workspace, _, _) = self.ready_worktree(&id).await?;
        if self.setup_gates.lock().contains_key(&id) {
            return Err(ApiError::new(ErrorCode::Conflict, "setup is already running"));
        }
        let repo_id = workspace.repo_id.clone();
        let repo = self.blocking(move |store| store.get_repo(&repo_id)).await?;
        Ok(self.start_setup(repo, workspace))
    }

    /// Stops the setup script, if one is running. The workspace stays usable.
    pub(crate) async fn cancel_setup(&self, workspace_id: &str) {
        if self.setup_gates.lock().contains_key(workspace_id)
            && let Some(link) = self.existing_worker(workspace_id)
        {
            let _ = link.call(WorkerRequest::CancelSetup).await;
        }
    }
}
