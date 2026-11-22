use super::super::ready::ReadyWorkspace;
use super::super::{Hub, new_id};
use super::naming;
use crate::error::{internal, provider_failed};
use crate::store::{Made, Store, WorkspacePlacement, now_ms};
use naming::{Names, name_taken};
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{
    AgentKind, BranchName, ClientRequestId, GithubRepositoryName, ProviderName, RecordKey, Repo, RepoId, RepoUrl, Rev,
    SetupStatus, TitleSource, Workspace, WorkspaceId, WorkspaceStatus,
};
use protocol::worker::{PrepareWorkspace, RepoSource, WorkspaceReady};
use providers::Provider;
use std::sync::Arc;
use tracing::{error, warn};

impl Hub {
    pub(crate) async fn workspace_create(
        self: &Arc<Self>,
        repo_id: RepoId,
        name: Option<String>,
        base_ref: Option<String>,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Workspace, ApiError> {
        self.make_workspace(repo_id, name, base_ref, client_request_id, None).await
    }

    /// Makes a workspace, as `WorkspaceCreate` asks, whose first chat runs `first_chat`, if it names one, whoever asks
    /// for that chat.
    pub(crate) async fn make_workspace(
        self: &Arc<Self>,
        repo_id: RepoId,
        name: Option<String>,
        base_ref: Option<String>,
        client_request_id: Option<ClientRequestId>,
        first_chat: Option<AgentKind>,
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
        let (provider_name, provider) = self.providers.for_new_workspace()?;
        // Resolve the placement (and refuse an unknown environment or missing remote address) before recording a
        // workspace or asking the provider for an instance.
        let placement = self.resolve_workspace_placement(&repo, provider.as_ref()).await?;
        let mut names = Names::new(name);
        let recorded =
            self.record_new_workspace(&repo, provider_name, &placement, &mut names, client_request_id, first_chat);
        let workspace = match recorded.await? {
            Made::New(workspace) => workspace,
            // Sent again while the first was making it.
            Made::Earlier(made) => return Ok(made),
        };
        let made = self
            .prepare_new_workspace(&repo, &placement.source, workspace.clone(), &mut names, base_ref, provider.as_ref())
            .await;
        if made.is_err() {
            self.discard_workspace(&workspace, provider.as_ref()).await;
        }
        made
    }

    /// Asks `provider` to create the instance already recorded as `Creating` for `workspace`, and marks it `Ready`.
    pub(super) async fn make_workspace_instance(
        &self,
        workspace: &Workspace,
        provider: &dyn Provider,
    ) -> Result<(), ApiError> {
        provider.create(&workspace.instance_id, None, None).await.map_err(|e| provider_failed(&e))?;
        let instance_id = workspace.instance_id.clone();
        self.blocking(move |store| store.instance_made(&instance_id)).await
    }

    /// Records a workspace in `repo`, being made, by the first of `names` that no other workspace there has, and
    /// announces it at once, before the fetch, which can take a while. So every client can show it on its way, where
    /// it will be, rather than working out from the list which new workspace is the one it asked for. One that can't
    /// be made is removed, which is announced too. Before it's announced, the hub takes `first_chat` as what its first
    /// chat runs.
    pub(super) async fn record_new_workspace(
        &self,
        repo: &Repo,
        provider_name: &ProviderName,
        placement: &WorkspacePlacement,
        names: &mut Names,
        client_request_id: Option<ClientRequestId>,
        first_chat: Option<AgentKind>,
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
            pull_request_link: None,
            comment_draft: None,
            branches: None,
            pull_request_branch: None,
            worker_build: None,
        };
        let mut taken = None;
        while let Some(name) = names.next(self, &repo.id).await? {
            (workspace.name, workspace.branch) = (name.clone(), crate::names::start_branch(&name));
            let (record, placement, request) = (workspace.clone(), placement.clone(), client_request_id.clone());
            match self.blocking(move |store| store.insert_workspace(&record, &placement, request.as_ref())).await? {
                Some(Made::New(made)) => {
                    if let Some(agent) = first_chat {
                        self.first_chats.lock().insert(made.id.clone(), agent);
                    }
                    self.notify(HubNotice::Workspace(made.clone()));
                    return Ok(Made::New(made));
                }
                Some(earlier) => return Ok(earlier),
                None => taken = Some(name_taken(&name, repo)),
            }
        }
        Err(taken.unwrap_or_else(|| internal("no names were tried")))
    }

    /// Makes the instance and the worktree for a workspace recorded as being made, and records it ready. A name git
    /// already has a directory, a branch or a folder of branches by is refused before anything is fetched, so the
    /// workspace tries the next of `names` in place, keeping its ID, and its place in every client's list, and its
    /// instance and worker, since an instance is known by its ID alone.
    pub(super) async fn prepare_new_workspace(
        self: &Arc<Self>,
        repo: &Repo,
        source: &RepoSource,
        mut workspace: Workspace,
        names: &mut Names,
        base_ref: Option<String>,
        provider: &dyn Provider,
    ) -> Result<Workspace, ApiError> {
        self.make_workspace_instance(&workspace, provider).await?;
        let link = self.ensure_worker(&workspace.id).await?;
        let reply = loop {
            let prepare = PrepareWorkspace {
                repo_name: repo.name.clone(),
                source: source.clone(),
                workspace_name: workspace.name.clone(),
                branch: workspace.branch.clone(),
                base_ref: base_ref.clone(),
            };
            let asked = match self.refuse_a_name_git_has(&link, repo, source, &workspace.name, names).await {
                Ok(()) => link.call(prepare).await,
                Err(refused) => Err(refused),
            };
            match asked {
                Err(e) if e.code == ErrorCode::NameTaken => {
                    workspace = self.rename_unready(workspace, names, e).await?;
                }
                asked => break asked?,
            }
        };
        let WorkspaceReady { path, branch, base_ref, warnings, branches } = reply;
        for warning in &warnings {
            warn!(workspace = workspace.name, "{warning}");
        }
        let id = workspace.id.clone();
        // Before it's ready on the record, since the reads nobody asks for start at once on a ready one with no schedule.
        self.nothing_to_read_yet(&id);
        let ready = self
            .blocking(move |store| store.workspace_ready(&id, &path, &branch, &base_ref, branches.as_ref()))
            .await?;
        let ready = ReadyWorkspace::new(ready)?;
        self.notify(HubNotice::Workspace(ready.workspace().clone()));
        // Once it's announced, so what the watch sees comes after.
        self.watch_branches(&ready, &link).await;
        self.start_setup(ready).await
    }

    /// Ends the workspace's worker, if it has one, and destroys its instance, whose record goes once it's gone, and
    /// then the workspace's record.
    pub(super) async fn discard_workspace(&self, workspace: &Workspace, provider: &dyn Provider) {
        let id = &workspace.id;
        self.first_chats.lock().remove(id);
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

    pub(crate) async fn repo_of(&self, workspace: &Workspace) -> Result<Repo, ApiError> {
        let repo_id = workspace.repo_id.clone();
        self.blocking(move |store| store.get_repo(&repo_id)).await
    }

    /// Which repository on GitHub the workspace's is, as its record's address names it, or `None` for one with no
    /// address on github.com.
    pub(crate) async fn github_repository_of(
        &self,
        workspace: &Workspace,
    ) -> Result<Option<GithubRepositoryName>, ApiError> {
        Ok(self.repo_of(workspace).await?.origin.url().and_then(RepoUrl::on_github))
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
