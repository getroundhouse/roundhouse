//! Making, archiving and restoring workspaces.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{
    ClientRequestId, RecordKey, Repo, RepoId, Rev, SetupStatus, TitleSource, Workspace, WorkspaceId, WorkspaceStatus,
};
use protocol::worker::{WorkerRequest, WorkerResponse};
use providers::Provider;
use tokio::sync::oneshot;
use tracing::{error, warn};

use super::{Hub, new_id, not_ready, unexpected};
use crate::error::{internal, provider_failed};
use crate::session::Command;
use crate::store::{Made, Store, now_ms};

/// What scripts and agents see, under the names Conductor uses, so that scripts written for it work.
pub(crate) fn workspace_env(workspace: &Workspace, repo: &Repo) -> Vec<(String, String)> {
    let base = workspace.base_ref.as_deref().unwrap_or("main");
    let mut env = vec![
        ("CONDUCTOR_WORKSPACE_NAME".to_string(), workspace.name.clone()),
        ("CONDUCTOR_WORKSPACE_ID".to_string(), workspace.id.to_string()),
        ("CONDUCTOR_ROOT_PATH".to_string(), repo.path.clone()),
        ("CONDUCTOR_DEFAULT_BRANCH".to_string(), base.strip_prefix("origin/").unwrap_or(base).to_string()),
        ("CONDUCTOR_PORT".to_string(), workspace.port_base.to_string()),
        ("CONDUCTOR_IS_LOCAL".to_string(), "1".to_string()),
    ];
    if let Some(path) = &workspace.path {
        env.push(("CONDUCTOR_WORKSPACE_PATH".to_string(), path.clone()));
    }
    env
}

impl Hub {
    pub(crate) async fn workspace_create(
        self: &Arc<Self>,
        repo_id: RepoId,
        name: Option<String>,
        base_ref: Option<String>,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Workspace, ApiError> {
        if let Some(made) = self.made_earlier(client_request_id.as_ref()).await? {
            return Ok(made);
        }
        let repo = {
            let repo_id = repo_id.clone();
            self.blocking(move |store| store.get_repo(&repo_id)).await?
        };
        if let Some(name) = &name
            && !crate::names::is_valid(name)
        {
            return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{name} isn't a valid workspace name")));
        }
        // A generated name can collide with a branch or directory the hub doesn't know about, so it
        // gets a few tries. A name the user chose gets one.
        let attempts = if name.is_some() { 1 } else { 8 };
        let mut last_error = None;
        for _ in 0..attempts {
            let chosen = match &name {
                Some(name) => name.clone(),
                None => self.next_name(&repo.id).await?,
            };
            match self.try_create(&repo, chosen, base_ref.clone(), client_request_id.clone()).await {
                Ok(workspace) => return Ok(workspace),
                Err(e) if e.code == ErrorCode::NameTaken => last_error = Some(e),
                Err(e) => return Err(e),
            }
        }
        Err(last_error.unwrap_or_else(|| internal("no attempts were made")))
    }

    async fn next_name(&self, repo_id: &RepoId) -> Result<String, ApiError> {
        let owned = repo_id.clone();
        let taken = self.blocking(move |store| store.names_in_repo(&owned)).await?;
        let salt = self.name_salt.fetch_add(1, Ordering::Relaxed);
        Ok(crate::names::pick(&taken, salt))
    }

    async fn try_create(
        self: &Arc<Self>,
        repo: &Repo,
        name: String,
        base_ref: Option<String>,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Workspace, ApiError> {
        let id = WorkspaceId::from(new_id());
        let (provider_name, provider) = self.providers.default_provider()?;
        let now = now_ms();
        let workspace = Workspace {
            id: id.clone(),
            repo_id: repo.id.clone(),
            name: name.clone(),
            branch: name.clone(),
            base_ref: None,
            path: None,
            status: WorkspaceStatus::Creating,
            provider: provider_name.to_string(),
            instance_id: id.as_str().into(),
            created_at_ms: now,
            archived_at_ms: None,
            // Making it is the first thing the person did in it.
            user_active_at_ms: Some(now),
            setup: SetupStatus::NotStarted,
            port_base: self.blocking(Store::next_port_base).await?,
            rev: Rev::default(),
            title: None,
            title_source: TitleSource::FirstLine,
            pull_request: None,
        };
        let record = workspace.clone();
        match self.blocking(move |store| store.insert_workspace(&record, client_request_id.as_ref())).await? {
            Some(Made::New(_)) => {}
            // Sent again while the first was making it.
            Some(Made::Earlier(made)) => return Ok(made),
            None => {
                return Err(ApiError::new(ErrorCode::NameTaken, format!("{name} is already taken in {}", repo.name)));
            }
        }
        let result = self.prepare_new_workspace(repo, &workspace, base_ref, provider.as_ref()).await;
        match result {
            Ok(ready) => Ok(ready),
            Err(e) => {
                self.discard_workspace(&workspace, provider.as_ref()).await;
                Err(e)
            }
        }
    }

    async fn prepare_new_workspace(
        self: &Arc<Self>,
        repo: &Repo,
        workspace: &Workspace,
        base_ref: Option<String>,
        provider: &dyn Provider,
    ) -> Result<Workspace, ApiError> {
        let display_name = format!("roundhouse-{}", workspace.name);
        provider.create(&workspace.instance_id, None, None, &display_name).await.map_err(|e| provider_failed(&e))?;
        let instance_id = workspace.instance_id.clone();
        self.blocking(move |store| store.instance_made(&instance_id)).await?;
        let link = self.ensure_worker(&workspace.id).await?;
        let reply = link
            .call(WorkerRequest::PrepareWorkspace {
                repo_name: repo.name.clone(),
                repo_path: repo.path.clone(),
                workspace_name: workspace.name.clone(),
                branch: workspace.branch.clone(),
                base_ref,
            })
            .await?;
        let WorkerResponse::WorkspaceReady { path, branch, base_ref, warnings } = reply else {
            return Err(unexpected(&reply));
        };
        for warning in &warnings {
            warn!(workspace = workspace.name, "{warning}");
        }
        let id = workspace.id.clone();
        let ready = self.blocking(move |store| store.workspace_ready(&id, &path, &branch, &base_ref)).await?;
        self.nothing_to_read_yet(&ready.id);
        self.notify(HubNotice::Workspace(ready.clone()));
        self.start_setup(repo.clone(), ready).await
    }

    async fn discard_workspace(&self, workspace: &Workspace, provider: &dyn Provider) {
        let id = &workspace.id;
        let link = self.workers.lock().get(id).cloned();
        if let Some(link) = link {
            self.close_worker(workspace, &link, provider).await;
        }
        if let Err(e) = self.destroy_instance(provider, &workspace.instance_id).await {
            warn!(%id, "cleaning up a failed workspace: {e}");
        }
        let owned = id.clone();
        match self.blocking(move |store| store.delete_workspace(&owned)).await {
            Ok(Some(rev)) => self.notify(HubNotice::Removed { record: RecordKey::Workspace { id: id.clone() }, rev }),
            Ok(None) => {}
            Err(e) => error!(%id, "removing a failed workspace: {e}"),
        }
    }

    /// Tells everyone following the feed what the workspace is now, read back from the store, after something with no
    /// notice of its own has changed it, such as the person sending one of its agents something. A failure is only
    /// logged, since what changed it has happened.
    pub(crate) async fn announce_workspace(&self, id: &WorkspaceId) {
        match self.workspace(id).await {
            Ok(workspace) => self.notify(HubNotice::Workspace(workspace)),
            Err(e) => error!(%id, "could not announce a change to a workspace: {e}"),
        }
    }

    pub(crate) async fn workspace_list(&self, include_archived: bool) -> Result<Vec<Workspace>, ApiError> {
        self.blocking(move |store| store.list_workspaces(include_archived)).await
    }

    pub(crate) async fn workspace_archive(
        self: &Arc<Self>,
        id: WorkspaceId,
        force: bool,
    ) -> Result<Workspace, ApiError> {
        let workspace = self.workspace(&id).await?;
        if workspace.status != WorkspaceStatus::Ready {
            return Err(not_ready(&workspace));
        }
        let repo = {
            let repo_id = workspace.repo_id.clone();
            self.blocking(move |store| store.get_repo(&repo_id)).await?
        };
        let path = workspace.path.clone().ok_or_else(|| internal("a ready workspace has no path"))?;
        let link = self.ensure_worker(&id).await?;

        // Before anything changes, so refusing leaves the workspace as it was.
        let status = link.call(WorkerRequest::GitStatus { path: path.clone() }).await;
        if !force
            && let Ok(WorkerResponse::GitStatus(status)) = &status
            && let Some(loss) = status.loss_if_removed()
        {
            return Err(ApiError::new(ErrorCode::NeedsForce, loss));
        }

        // Interrupt setup first, so the script isn't running in a worktree that's about to go.
        let _ = link.call(WorkerRequest::CancelSetup).await;
        let script = WorkerRequest::ArchiveScript {
            repo_path: repo.path.clone(),
            path: path.clone(),
            env: workspace_env(&workspace, &repo),
        };
        match link.call(script).await {
            Ok(_) => {}
            Err(e) if !force => return Err(e),
            Err(e) => warn!(workspace = workspace.name, "the archive script failed: {}", e.message),
        }

        self.stop_workspace_agents(&id).await?;
        // Checks again unless forced, because the agent may have written files before it stopped.
        let reply = link.call(WorkerRequest::RemoveWorkspace { repo_path: repo.path, path, force }).await?;
        if !matches!(reply, WorkerResponse::Ok) {
            return Err(unexpected(&reply));
        }
        let provider = self.provider(&workspace.provider)?;
        self.close_worker(&workspace, &link, provider.as_ref()).await;
        // Agents are expected to rename their branch to match the work. Restoring needs its real name.
        let branch = match status {
            Ok(WorkerResponse::GitStatus(status)) => status.branch,
            _ => None,
        };
        let archived = id.clone();
        self.blocking(move |store| {
            if let Some(branch) = branch {
                store.set_workspace_branch(&archived, &branch)?;
            }
            store.set_workspace_archived(&archived, true)
        })
        .await
        .inspect(|workspace| self.notify(HubNotice::Workspace(workspace.clone())))
    }

    async fn stop_workspace_agents(self: &Arc<Self>, workspace_id: &WorkspaceId) -> Result<(), ApiError> {
        let sessions: Vec<_> = {
            let owned = workspace_id.clone();
            self.blocking(move |store| store.list_sessions(&owned)).await?
        };
        for session in sessions {
            let live = self.live_session(&session.id).await?;
            if !live.agent_running() {
                continue;
            }
            let (reply, done) = oneshot::channel();
            live.command(Command::Stop { reply }).await?;
            done.await.map_err(|_| internal("the session has stopped"))??;
        }
        Ok(())
    }

    pub(crate) async fn workspace_restore(self: &Arc<Self>, id: WorkspaceId) -> Result<Workspace, ApiError> {
        let workspace = self.workspace(&id).await?;
        if workspace.status != WorkspaceStatus::Archived {
            return Err(ApiError::new(ErrorCode::Conflict, format!("workspace {} isn't archived", workspace.name)));
        }
        let repo = {
            let repo_id = workspace.repo_id.clone();
            self.blocking(move |store| store.get_repo(&repo_id)).await?
        };
        let path = workspace.path.clone().ok_or_else(|| internal("an archived workspace has no path"))?;
        let link = self.ensure_worker(&id).await?;
        let setup_repo = repo.clone();
        let reply =
            link.call(WorkerRequest::RestoreWorkspace { repo_path: repo.path, path, branch: workspace.branch }).await?;
        if !matches!(reply, WorkerResponse::Ok) {
            return Err(unexpected(&reply));
        }
        let restored = self.blocking(move |store| store.set_workspace_archived(&id, false)).await?;
        self.notify(HubNotice::Workspace(restored.clone()));
        // A restored worktree has lost what setup made, since none of it is committed.
        self.start_setup(setup_repo, restored).await
    }
}
