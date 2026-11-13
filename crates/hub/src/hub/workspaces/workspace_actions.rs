use super::super::ready::ReadyWorkspace;
use super::super::{Hub, new_id};
use super::naming;
use crate::error::{internal, provider_failed};
use crate::store::{Made, Store, now_ms};
use naming::{Names, name_taken};
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{
    BranchName, ClientRequestId, ProviderName, RecordKey, Repo, RepoId, Rev, SetupStatus, TitleSource, Workspace,
    WorkspaceId, WorkspaceStatus,
};
use protocol::worker::{PrepareWorkspace, WorkspaceReady};
use providers::Provider;
use std::sync::Arc;
use tracing::{error, warn};

/// What scripts and agents see, under the names Conductor uses, so that scripts written for it work.
pub(crate) fn workspace_env(workspace: &Workspace, repo: &Repo) -> Vec<(String, String)> {
    let base = workspace.base_ref.as_deref().unwrap_or("main");
    let mut env = vec![
        ("CONDUCTOR_WORKSPACE_NAME".to_string(), workspace.name.clone()),
        ("CONDUCTOR_WORKSPACE_ID".to_string(), workspace.id.to_string()),
        ("CONDUCTOR_ROOT_PATH".to_string(), repo.path.clone()),
        ("CONDUCTOR_DEFAULT_BRANCH".to_string(), base.trim_prefix("origin/").to_string()),
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
        let (provider_name, provider) = self.providers.default_provider()?;
        let mut names = Names::new(name);
        let workspace = match self.record_new_workspace(&repo, provider_name, &mut names, client_request_id).await? {
            Made::New(workspace) => workspace,
            // Sent again while the first was making it.
            Made::Earlier(made) => return Ok(made),
        };
        let made = self.prepare_new_workspace(&repo, workspace.clone(), &mut names, base_ref, provider.as_ref()).await;
        if made.is_err() {
            self.discard_workspace(&workspace, provider.as_ref()).await;
        }
        made
    }

    /// Records a workspace in `repo`, being made, by the first of `names` that no other workspace there has, and
    /// announces it at once, before the fetch, which can take a while. So every client can show it on its way, where
    /// it will be, rather than working out from the list which new workspace is the one it asked for. One that can't
    /// be made is removed, which is announced too.
    pub(super) async fn record_new_workspace(
        &self,
        repo: &Repo,
        provider_name: &ProviderName,
        names: &mut Names,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Made<Workspace>, ApiError> {
        let id = WorkspaceId::from(new_id());
        let now = now_ms();
        let mut workspace = Workspace {
            id: id.clone(),
            repo_id: repo.id.clone(),
            name: String::new(),
            branch: BranchName::from(""),
            base_ref: None,
            path: None,
            status: WorkspaceStatus::Creating,
            provider: provider_name.clone(),
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
            comment_draft: None,
            branches: None,
            pull_request_branch: None,
            worker_build: None,
        };
        let mut taken = None;
        while let Some(name) = names.next(self, &repo.id).await? {
            (workspace.name, workspace.branch) = (name.clone(), crate::names::start_branch(&name));
            let (record, request) = (workspace.clone(), client_request_id.clone());
            match self.blocking(move |store| store.insert_workspace(&record, request.as_ref())).await? {
                Some(Made::New(made)) => {
                    self.notify(HubNotice::Workspace(made.clone()));
                    return Ok(Made::New(made));
                }
                Some(earlier) => return Ok(earlier),
                None => taken = Some(name_taken(&name, repo)),
            }
        }
        Err(taken.unwrap_or_else(|| internal("no names were tried")))
    }

    /// Makes the worktree for a workspace recorded as being made, and records it ready. A name git already has a
    /// directory, a branch or a folder of branches by is refused before anything is fetched or made, so the workspace
    /// tries the next of `names` in place, keeping its ID, and its place in every client's list. Its instance is made
    /// again under each new name, since the provider's own tools call it by the name it was made with, and recorded as
    /// it is.
    pub(super) async fn prepare_new_workspace(
        self: &Arc<Self>,
        repo: &Repo,
        mut workspace: Workspace,
        names: &mut Names,
        base_ref: Option<String>,
        provider: &dyn Provider,
    ) -> Result<Workspace, ApiError> {
        let (reply, link) = loop {
            let display_name = format!("roundhouse-{}", workspace.name);
            provider
                .create(&workspace.instance_id, None, None, &display_name)
                .await
                .map_err(|e| provider_failed(&e))?;
            let instance_id = workspace.instance_id.clone();
            self.blocking(move |store| store.instance_made(&instance_id)).await?;
            let link = self.ensure_worker(&workspace.id).await?;
            let prepare = PrepareWorkspace {
                repo_name: repo.name.clone(),
                repo_path: repo.path.clone(),
                workspace_name: workspace.name.clone(),
                branch: workspace.branch.clone(),
                base_ref: base_ref.clone(),
            };
            let asked = match self.refuse_a_name_git_has(&link, repo, &workspace.name, names).await {
                Ok(()) => link.call(prepare).await,
                Err(refused) => Err(refused),
            };
            match asked {
                Err(e) if e.code == ErrorCode::NameTaken => {
                    workspace = self.rename_unready(workspace, names, e).await?;
                    self.unmake_instance(&workspace, provider).await?;
                    // Recorded before its provider is asked for it again, as it was with the workspace.
                    let (instance_id, provider_name) = (workspace.instance_id.clone(), workspace.provider.clone());
                    self.blocking(move |store| store.instance_creating(&instance_id, &provider_name)).await?;
                }
                asked => break (asked?, link),
            }
        };
        let WorkspaceReady { path, branch, base_ref, warnings, branches } = reply;
        for warning in &warnings {
            warn!(workspace = workspace.name, "{warning}");
        }
        let id = workspace.id.clone();
        let ready = self
            .blocking(move |store| store.workspace_ready(&id, &path, &branch, &base_ref, branches.as_ref()))
            .await?;
        let ready = ReadyWorkspace::new(ready)?;
        self.nothing_to_read_yet(ready.id());
        self.notify(HubNotice::Workspace(ready.workspace().clone()));
        // Once it's announced, so what the watch sees comes after.
        self.watch_branches(&ready, &link).await;
        self.start_setup(repo.clone(), ready).await
    }

    /// Ends the workspace's worker, if it has one, and destroys its instance, whose record goes once it's gone.
    pub(super) async fn unmake_instance(&self, workspace: &Workspace, provider: &dyn Provider) -> Result<(), ApiError> {
        let link = self.workers.lock().get(&workspace.id).cloned();
        if let Some(link) = link {
            self.close_worker(workspace, &link, provider).await;
        }
        self.destroy_instance(provider, &workspace.instance_id).await
    }

    pub(super) async fn discard_workspace(&self, workspace: &Workspace, provider: &dyn Provider) {
        let id = &workspace.id;
        if let Err(e) = self.unmake_instance(workspace, provider).await {
            warn!(%id, "cleaning up a failed workspace: {e}");
        }
        let owned = id.clone();
        match self.blocking(move |store| store.delete_workspace(&owned)).await {
            Ok(Some(rev)) => self.notify(HubNotice::Removed { record: RecordKey::Workspace { id: id.clone() }, rev }),
            Ok(None) => {}
            Err(e) => error!(%id, "removing a failed workspace: {e}"),
        }
    }

    pub(crate) async fn repo_of(&self, workspace: &Workspace) -> Result<Repo, ApiError> {
        let repo_id = workspace.repo_id.clone();
        self.blocking(move |store| store.get_repo(&repo_id)).await
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
}
