//! A hub owns state and serves the platform API. It doesn't do the work itself: workers do, and
//! providers say where they run.

#![warn(clippy::wildcard_enum_match_arm)]

mod error;
mod names;
mod server;
mod session;
pub mod store;
mod worker_link;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use protocol::PROTOCOL_VERSION;
use protocol::client::{ApiError, ClientResponse, ErrorCode, HubNotice, HubStatus, error_chain};
use protocol::event::{ApprovalDecision, EventBody, ExitReason, SessionEvent};
use protocol::model::{
    AgentKind, DiffComment, DiffSide, GitDiff, GitStatus, PullRequest, Repo, Seq, Session, SessionStatus, SetupStatus,
    Workspace, WorkspaceId, WorkspaceStatus,
};
use protocol::paths::Paths;
use protocol::worker::{WorkerRequest, WorkerResponse};
use providers::Provider;
use providers::local::{LocalConfig, LocalProvider};
use session::{Command, Item, LiveSession};
use store::{Store, StoreError, StoredSession, cancelled_approval, now_ms};

use crate::error::{internal, provider_failed};

pub use crate::error::HubError;
use tokio::sync::{Mutex as AsyncMutex, Notify, broadcast, oneshot, watch};
use tracing::{error, info, warn};
use worker_link::WorkerLink;

pub struct HubConfig {
    pub paths: Paths,
    /// The binary that runs `worker --stdio`.
    pub worker_exe: PathBuf,
    /// The user's shell, for finding the real `PATH`.
    pub shell: Option<String>,
    /// This process's own `PATH`.
    pub path: Option<String>,
}

const LOCAL_PROVIDER: &str = "local";
/// Status changes are rare, so a client only overflows this if it has stopped reading.
const FEED_CAPACITY: usize = 256;
/// Agents get 3 s to stop and the worker flushes after that, so this is only reached if something
/// is stuck.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(15);

pub struct Hub {
    store: Arc<Store>,
    providers: HashMap<String, Arc<dyn Provider>>,
    default_provider: String,
    workers: Mutex<HashMap<WorkspaceId, Arc<WorkerLink>>>,
    /// Held while a workspace's worker starts or is cleaned up, so an instance never has two. One per
    /// workspace, because a slow or stuck worker mustn't hold up the others.
    worker_gates: Mutex<HashMap<WorkspaceId, Arc<AsyncMutex<()>>>>,
    /// Held while finding or making a workspace's first session.
    ensuring_session: AsyncMutex<()>,
    sessions: Mutex<HashMap<String, Arc<LiveSession>>>,
    /// Workspaces whose setup is running. Removed as it finishes, which also wakes anyone waiting.
    setup_gates: Mutex<HashMap<WorkspaceId, watch::Sender<bool>>>,
    feed: broadcast::Sender<HubNotice>,
    started_at_ms: i64,
    shutdown: Notify,
    shutting_down: AtomicBool,
    name_salt: AtomicU64,
    next_run: AtomicU64,
    clients: AtomicU32,
}

fn not_ready(workspace: &Workspace) -> ApiError {
    ApiError::new(ErrorCode::Conflict, format!("workspace {} is {:?}", workspace.name, workspace.status))
}

fn unexpected(reply: &WorkerResponse) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("unexpected reply from the worker: {reply:?}"))
}

impl Hub {
    fn provider(&self, name: &str) -> Result<Arc<dyn Provider>, ApiError> {
        self.providers.get(name).cloned().ok_or_else(|| internal(format!("no provider {name}")))
    }

    async fn blocking<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&Store) -> Result<T, StoreError> + Send + 'static,
    {
        let store = self.store.clone();
        let result = tokio::task::spawn_blocking(move || f(&store)).await.map_err(|e| internal(e.to_string()))?;
        Ok(result?)
    }

    pub(crate) async fn workspace(&self, id: &str) -> Result<Workspace, ApiError> {
        let id = id.to_string();
        self.blocking(move |store| store.get_workspace(&id)).await
    }

    /// A number no agent launched by this hub has used. Starts above the "no run" marker.
    pub(crate) fn next_run(&self) -> u64 {
        self.next_run.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) fn subscribe_feed(&self) -> broadcast::Receiver<HubNotice> {
        self.feed.subscribe()
    }

    pub(crate) fn notify(&self, notice: HubNotice) {
        // An error only means nobody is listening.
        let _ = self.feed.send(notice);
    }

    fn worker_gate(&self, workspace_id: &str) -> Arc<AsyncMutex<()>> {
        self.worker_gates.lock().entry(workspace_id.to_string()).or_default().clone()
    }

    /// The running worker, if there is one.
    pub(crate) fn existing_worker(&self, workspace_id: &str) -> Option<Arc<WorkerLink>> {
        self.workers.lock().get(workspace_id).filter(|link| link.is_open()).cloned()
    }

    /// The workspace's worker, starting it if needed.
    pub(crate) async fn ensure_worker(self: &Arc<Self>, workspace_id: &str) -> Result<Arc<WorkerLink>, ApiError> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(ApiError::new(ErrorCode::Conflict, "the hub is shutting down"));
        }
        if let Some(link) = self.existing_worker(workspace_id) {
            return Ok(link);
        }
        let workspace = self.workspace(workspace_id).await?;
        let gate = self.worker_gate(workspace_id);
        let _starting = gate.lock().await;
        if let Some(link) = self.existing_worker(workspace_id) {
            return Ok(link);
        }
        let provider = self.provider(&workspace.provider)?;
        let link = WorkerLink::open(self.clone(), provider, workspace.id.clone(), workspace.instance_id).await?;
        self.workers.lock().insert(workspace.id, link.clone());
        Ok(link)
    }

    /// Called when a worker's stream ends, after everything it sent has been queued.
    pub(crate) async fn worker_gone(
        &self,
        workspace_id: &str,
        link: &Arc<WorkerLink>,
        provider: &dyn Provider,
        instance_id: &str,
    ) {
        let gate = self.worker_gate(workspace_id);
        let _cleaning_up = gate.lock().await;
        {
            let mut workers = self.workers.lock();
            if workers.get(workspace_id).is_some_and(|current| Arc::ptr_eq(current, link)) {
                workers.remove(workspace_id);
            }
        }
        if let Err(e) = provider.stop(instance_id).await {
            warn!(instance_id, "stopping a worker failed: {}", error_chain(&e));
        }
        // Still under the gate, so these are queued before a replacement worker can start.
        let sessions: Vec<_> =
            self.sessions.lock().values().filter(|session| session.workspace_id == workspace_id).cloned().collect();
        for session in sessions {
            session.send_item(Item::WorkerLost(link.id)).await;
        }
    }

    /// Output for a session goes through here on its way to the log.
    pub(crate) async fn route(self: &Arc<Self>, session_id: &str, item: Item) {
        match self.live_session(session_id).await {
            Ok(session) => session.send_item(item).await,
            Err(e) => warn!(session_id, "dropping output for an unknown session: {e}"),
        }
    }

    pub(crate) async fn live_session(self: &Arc<Self>, id: &str) -> Result<Arc<LiveSession>, ApiError> {
        if let Some(session) = self.sessions.lock().get(id) {
            return Ok(session.clone());
        }
        let owned = id.to_string();
        let (stored, approvals) = self
            .blocking(move |store| {
                let stored = store.get_session(&owned)?;
                let approvals = store.open_approvals(&owned)?;
                Ok((stored, approvals))
            })
            .await?;
        let mut sessions = self.sessions.lock();
        let session = sessions.entry(id.to_string()).or_insert_with(|| LiveSession::start(self, stored, approvals));
        Ok(session.clone())
    }

    // Platform API --------------------------------------------------------------------------------

    pub(crate) fn request_shutdown(&self) {
        self.shutdown.notify_one();
    }

    pub(crate) fn status(&self) -> HubStatus {
        let running_sessions = self
            .sessions
            .lock()
            .values()
            .filter(|session| matches!(session.status(), SessionStatus::Running | SessionStatus::WaitingApproval))
            .count();
        HubStatus {
            pid: std::process::id(),
            hub_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION,
            started_at_ms: self.started_at_ms,
            workers: u32::try_from(self.workers.lock().len()).unwrap_or(u32::MAX),
            running_sessions: u32::try_from(running_sessions).unwrap_or(u32::MAX),
            clients: self.clients.load(Ordering::Relaxed),
        }
    }

    pub(crate) async fn repo_add(&self, path: String) -> Result<Repo, ApiError> {
        let root = find_repo_root(Path::new(&path))?;
        let name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| ApiError::new(ErrorCode::InvalidRequest, format!("{path} has no directory name")))?;
        let repo = Repo { id: new_id(), name, path: root.to_string_lossy().into_owned(), created_at_ms: now_ms() };
        let repo = self.blocking(move |store| store.add_repo(&repo)).await?;
        self.notify(HubNotice::Repo(repo.clone()));
        Ok(repo)
    }

    pub(crate) async fn repo_list(&self) -> Result<Vec<Repo>, ApiError> {
        self.blocking(Store::list_repos).await
    }

    pub(crate) async fn workspace_create(
        self: &Arc<Self>,
        repo_id: String,
        name: Option<String>,
        base_ref: Option<String>,
    ) -> Result<Workspace, ApiError> {
        let repo = {
            let repo_id = repo_id.clone();
            self.blocking(move |store| store.get_repo(&repo_id)).await?
        };
        if let Some(name) = &name
            && !names::is_valid(name)
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
            match self.try_create(&repo, chosen, base_ref.clone()).await {
                Ok(workspace) => return Ok(workspace),
                Err(e) if e.code == ErrorCode::Conflict => last_error = Some(e),
                Err(e) => return Err(e),
            }
        }
        Err(last_error.unwrap_or_else(|| internal("no attempts were made")))
    }

    async fn next_name(&self, repo_id: &str) -> Result<String, ApiError> {
        let owned = repo_id.to_string();
        let taken = self.blocking(move |store| store.names_in_repo(&owned)).await?;
        let salt = self.name_salt.fetch_add(1, Ordering::Relaxed);
        Ok(names::pick(&taken, salt))
    }

    async fn try_create(
        self: &Arc<Self>,
        repo: &Repo,
        name: String,
        base_ref: Option<String>,
    ) -> Result<Workspace, ApiError> {
        let id = new_id();
        let provider = self.provider(&self.default_provider)?;
        let workspace = Workspace {
            id: id.clone(),
            repo_id: repo.id.clone(),
            name: name.clone(),
            branch: name.clone(),
            base_ref: None,
            path: None,
            status: WorkspaceStatus::Creating,
            provider: self.default_provider.clone(),
            instance_id: id.clone(),
            created_at_ms: now_ms(),
            archived_at_ms: None,
            setup: SetupStatus::NotStarted,
            port_base: self.blocking(Store::next_port_base).await?,
        };
        let record = workspace.clone();
        if !self.blocking(move |store| store.insert_workspace(&record)).await? {
            return Err(ApiError::new(ErrorCode::Conflict, format!("{name} is already taken in {}", repo.name)));
        }
        let result = self.prepare_new_workspace(repo, &workspace, base_ref, provider.as_ref()).await;
        match result {
            Ok(ready) => Ok(ready),
            Err(e) => {
                self.discard_workspace(&id, provider.as_ref()).await;
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
        provider.create(&workspace.instance_id).await.map_err(|e| provider_failed(&e))?;
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
        self.notify(HubNotice::Workspace(ready.clone()));
        Ok(self.start_setup(repo.clone(), ready))
    }

    async fn discard_workspace(&self, id: &str, provider: &dyn Provider) {
        let link = self.workers.lock().get(id).cloned();
        if let Some(link) = link {
            close_worker(&link, provider, id).await;
        }
        if let Err(e) = provider.destroy(id).await {
            warn!(id, "cleaning up a failed workspace: {}", error_chain(&e));
        }
        let owned = id.to_string();
        if let Err(e) = self.blocking(move |store| store.delete_workspace(&owned)).await {
            error!(id, "removing a failed workspace: {e}");
        }
    }

    pub(crate) async fn workspace_list(&self, include_archived: bool) -> Result<Vec<Workspace>, ApiError> {
        self.blocking(move |store| store.list_workspaces(include_archived)).await
    }

    pub(crate) async fn workspace_archive(self: &Arc<Self>, id: String, force: bool) -> Result<Workspace, ApiError> {
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

        // Check before stopping anything, so a refused archive leaves the agent running.
        let status = link.call(WorkerRequest::GitStatus { path: path.clone() }).await;
        if !force
            && let Ok(WorkerResponse::GitStatus(status)) = &status
            && !status.files.is_empty()
        {
            return Err(ApiError::new(
                ErrorCode::Conflict,
                format!(
                    "{} uncommitted change(s) in {} would be lost. Commit them, or archive with force",
                    status.files.len(),
                    workspace.name
                ),
            ));
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
        close_worker(&link, provider.as_ref(), &workspace.instance_id).await;
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

    async fn stop_workspace_agents(self: &Arc<Self>, workspace_id: &str) -> Result<(), ApiError> {
        let sessions: Vec<_> = {
            let owned = workspace_id.to_string();
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

    pub(crate) async fn workspace_restore(self: &Arc<Self>, id: String) -> Result<Workspace, ApiError> {
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
        Ok(self.start_setup(setup_repo, restored))
    }

    async fn ready_worktree(self: &Arc<Self>, id: &str) -> Result<(Workspace, String, Arc<WorkerLink>), ApiError> {
        let workspace = self.workspace(id).await?;
        if workspace.status != WorkspaceStatus::Ready {
            return Err(not_ready(&workspace));
        }
        let path = workspace.path.clone().ok_or_else(|| internal("a ready workspace has no path"))?;
        let link = self.ensure_worker(id).await?;
        Ok((workspace, path, link))
    }

    pub(crate) async fn workspace_status(self: &Arc<Self>, id: String) -> Result<GitStatus, ApiError> {
        let (_, path, link) = self.ready_worktree(&id).await?;
        let reply = link.call(WorkerRequest::GitStatus { path }).await?;
        let WorkerResponse::GitStatus(status) = reply else {
            return Err(unexpected(&reply));
        };
        Ok(status)
    }

    pub(crate) async fn workspace_diff(self: &Arc<Self>, id: String) -> Result<GitDiff, ApiError> {
        let (workspace, path, link) = self.ready_worktree(&id).await?;
        let reply = link.call(WorkerRequest::GitDiff { path, base_ref: workspace.base_ref }).await?;
        let WorkerResponse::GitDiff(diff) = reply else {
            return Err(unexpected(&reply));
        };
        Ok(diff)
    }

    /// Starts setup in the background, and returns the workspace as it now stands. Prompts wait for it.
    fn start_setup(self: &Arc<Self>, repo: Repo, workspace: Workspace) -> Workspace {
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

    pub(crate) async fn workspace_pull_request(self: &Arc<Self>, id: String) -> Result<Option<PullRequest>, ApiError> {
        let (_, path, link) = self.ready_worktree(&id).await?;
        let reply = link.call(WorkerRequest::PullRequest { path }).await?;
        let WorkerResponse::PullRequest { pull_request } = reply else {
            return Err(unexpected(&reply));
        };
        Ok(pull_request)
    }

    pub(crate) async fn comment_add(
        &self,
        workspace_id: String,
        path: String,
        side: DiffSide,
        line: u32,
        text: String,
    ) -> Result<DiffComment, ApiError> {
        if text.trim().is_empty() {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "a comment can't be empty"));
        }
        self.workspace(&workspace_id).await?;
        let comment = DiffComment { id: new_id(), workspace_id, path, side, line, text, created_at_ms: now_ms() };
        let stored = comment.clone();
        self.blocking(move |store| store.add_comment(&stored)).await?;
        Ok(comment)
    }

    pub(crate) async fn comment_list(&self, workspace_id: String) -> Result<Vec<DiffComment>, ApiError> {
        self.blocking(move |store| store.list_comments(&workspace_id)).await
    }

    pub(crate) async fn comment_delete(&self, id: String) -> Result<(), ApiError> {
        self.blocking(move |store| store.delete_comment(&id)).await
    }

    pub(crate) async fn session_create(
        self: &Arc<Self>,
        workspace_id: String,
        agent: AgentKind,
        model: Option<String>,
        permission_mode: Option<String>,
    ) -> Result<Session, ApiError> {
        let workspace = self.workspace(&workspace_id).await?;
        if workspace.status != WorkspaceStatus::Ready {
            return Err(not_ready(&workspace));
        }
        let id = new_id();
        let now = now_ms();
        let stored = StoredSession {
            session: Session {
                id: id.clone(),
                workspace_id,
                agent,
                model,
                permission_mode,
                status: SessionStatus::Idle,
                last_seq: 0,
                created_at_ms: now,
                updated_at_ms: now,
                title: None,
                closed_at_ms: None,
            },
            // The hub's ID doubles as the agent's, so `--resume` works after a crash even if the
            // agent never reported one.
            agent_session_id: id,
            agent_opened: false,
        };
        let session = stored.session.clone();
        self.blocking(move |store| store.insert_session(&stored)).await?;
        self.notify(HubNotice::Session(session.clone()));
        Ok(session)
    }

    pub(crate) async fn session_ensure(
        self: &Arc<Self>,
        workspace_id: String,
        agent: AgentKind,
    ) -> Result<Session, ApiError> {
        let _one_at_a_time = self.ensuring_session.lock().await;
        let sessions = self.session_list(workspace_id.clone()).await?;
        match sessions.into_iter().rfind(|s| s.closed_at_ms.is_none()) {
            Some(open) => Ok(open),
            None => self.session_create(workspace_id, agent, None, None).await,
        }
    }

    pub(crate) async fn session_list(&self, workspace_id: String) -> Result<Vec<Session>, ApiError> {
        self.blocking(move |store| store.list_sessions(&workspace_id)).await
    }

    pub(crate) async fn session_get(&self, id: String) -> Result<Session, ApiError> {
        self.blocking(move |store| store.get_session(&id).map(|stored| stored.session)).await
    }

    pub(crate) async fn session_read(
        &self,
        id: String,
        from_seq: Seq,
        limit: u32,
        max_bytes: Option<u64>,
        include_raw: bool,
    ) -> Result<ClientResponse, ApiError> {
        let max_bytes = max_bytes.unwrap_or(server::DEFAULT_READ_BYTES);
        let limit = limit.clamp(1, server::MAX_READ_EVENTS);
        self.blocking(move |store| {
            let last_seq = store.get_session(&id)?.session.last_seq;
            let events = store.read_events(&id, from_seq, limit, max_bytes, include_raw)?;
            Ok(ClientResponse::Events { events, last_seq })
        })
        .await
    }

    /// Reads for a subscriber, never past `through`.
    pub(crate) async fn read_page(
        &self,
        id: String,
        from_seq: Seq,
        through: Seq,
    ) -> Result<Vec<SessionEvent>, ApiError> {
        let limit =
            u32::try_from(through - from_seq + 1).map_or(server::PUMP_PAGE_EVENTS, |n| n.min(server::PUMP_PAGE_EVENTS));
        self.blocking(move |store| store.read_events(&id, from_seq, limit, server::DEFAULT_READ_BYTES, false)).await
    }

    pub(crate) async fn session_prompt(
        self: &Arc<Self>,
        id: String,
        text: String,
        client_msg_id: Option<String>,
    ) -> Result<(Seq, bool), ApiError> {
        if text.trim().is_empty() {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "a prompt can't be empty"));
        }
        let session = self.live_session(&id).await?;
        let (reply, recorded) = oneshot::channel();
        session.command(Command::Prompt { text, client_msg_id, reply }).await?;
        recorded.await.map_err(|_| internal("the session has stopped"))?
    }

    pub(crate) async fn session_approve(
        self: &Arc<Self>,
        id: String,
        approval_id: String,
        decision: ApprovalDecision,
    ) -> Result<(), ApiError> {
        let session = self.live_session(&id).await?;
        let (reply, done) = oneshot::channel();
        session.command(Command::Approve { approval_id, decision, reply }).await?;
        done.await.map_err(|_| internal("the session has stopped"))?
    }

    pub(crate) async fn session_configure(
        self: &Arc<Self>,
        id: String,
        model: Option<String>,
        permission_mode: Option<String>,
    ) -> Result<Session, ApiError> {
        // Not through the session's queue, because a prompt waiting on setup holds that. The choice
        // has to apply when that prompt finally starts the agent.
        self.live_session(&id).await?.configure(model, permission_mode).await
    }

    pub(crate) async fn session_close(self: &Arc<Self>, id: String) -> Result<Session, ApiError> {
        self.live_session(&id).await?.close().await
    }

    pub(crate) async fn session_reopen(self: &Arc<Self>, id: String) -> Result<Session, ApiError> {
        self.live_session(&id).await?.reopen().await
    }

    pub(crate) async fn session_cancel(self: &Arc<Self>, id: String) -> Result<(), ApiError> {
        let session = self.live_session(&id).await?;
        // A prompt waiting on setup holds this session's commands, so stopping has to reach setup directly.
        self.cancel_setup(&session.workspace_id).await;
        let (reply, done) = oneshot::channel();
        session.command(Command::Cancel { reply }).await?;
        done.await.map_err(|_| internal("the session has stopped"))?
    }

    /// Stops every worker and waits until what they said is recorded.
    async fn stop_all_workers(&self) {
        // Refuse new workers first, so a client still connected can't start one behind this.
        self.shutting_down.store(true, Ordering::SeqCst);
        let links: Vec<_> = self.workers.lock().values().cloned().collect();
        for link in &links {
            link.close();
        }
        // One deadline for all of them, so several stuck workers don't add up.
        let deadline = tokio::time::Instant::now() + SHUTDOWN_GRACE;
        for link in &links {
            if tokio::time::timeout_at(deadline, link.wait_closed()).await.is_err() {
                warn!("a worker didn't stop in time");
            }
        }
        let sessions: Vec<_> = self.sessions.lock().values().cloned().collect();
        for session in sessions {
            let (done, flushed) = oneshot::channel();
            session.send_item(Item::Flush(done)).await;
            let _ = flushed.await;
        }
    }
}

/// Asks a worker to stop and waits for its output to end. If it's stuck, the provider ends it.
async fn close_worker(link: &WorkerLink, provider: &dyn Provider, instance_id: &str) {
    link.close();
    if tokio::time::timeout(SHUTDOWN_GRACE, link.wait_closed()).await.is_err() {
        warn!(instance_id, "a worker didn't stop in time");
        if let Err(e) = provider.stop(instance_id).await {
            warn!(instance_id, "stopping a stuck worker failed: {}", error_chain(&e));
        }
    }
}

/// What scripts and agents see, under the names Conductor uses, so that scripts written for it work.
pub(crate) fn workspace_env(workspace: &Workspace, repo: &Repo) -> Vec<(String, String)> {
    let base = workspace.base_ref.as_deref().unwrap_or("main");
    let mut env = vec![
        ("CONDUCTOR_WORKSPACE_NAME".to_string(), workspace.name.clone()),
        ("CONDUCTOR_WORKSPACE_ID".to_string(), workspace.id.clone()),
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

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn find_repo_root(path: &Path) -> Result<PathBuf, ApiError> {
    let canonical = path
        .canonicalize()
        .map_err(|e| ApiError::new(ErrorCode::InvalidRequest, format!("{}: {e}", path.display())))?;
    canonical.ancestors().find(|dir| dir.join(".git").exists()).map(Path::to_path_buf).ok_or_else(|| {
        ApiError::new(ErrorCode::InvalidRequest, format!("{} isn't in a git repository", path.display()))
    })
}

/// Anything left mid-turn or half-made by the last run. Its workers exited when it went away.
fn recover(store: &Store) -> Result<Vec<Workspace>, StoreError> {
    store.fail_interrupted_setups()?;
    for id in store.active_session_ids()? {
        for approval_id in store.open_approvals(&id)? {
            store.append(&id, &cancelled_approval(approval_id), None, None, None)?;
        }
        store.append(
            &id,
            &EventBody::AgentExited { reason: ExitReason::HubRestart, code: None, signal: None, stderr_tail: vec![] },
            None,
            None,
            Some(SessionStatus::Interrupted),
        )?;
    }
    store.take_unfinished_workspaces()
}

/// Only sets the mode on directories it creates. `TRESTLE_HOME` can point anywhere, and changing the
/// permissions of an existing directory could break something unrelated. The files that matter get
/// their own restrictive modes.
fn private_dir(path: &Path) -> Result<(), HubError> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|e| HubError::io(format!("creating {}", path.display()), e))
}

/// SQLite gives its -wal and -shm files the same mode as the database.
fn private_file(path: &Path) -> Result<(), HubError> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .map(drop)
        .map_err(|e| HubError::io(format!("creating {}", path.display()), e))
}

/// Runs until asked to stop or signalled. Returns an error if another hub is using these paths.
pub async fn run(config: HubConfig) -> Result<(), HubError> {
    let paths = config.paths;
    private_dir(&paths.data_dir)?;
    let lock_file = std::fs::File::create(&paths.lock).map_err(|e| HubError::io("opening the hub lock", e))?;
    if let Err(e) = lock_file.try_lock() {
        match e {
            std::fs::TryLockError::WouldBlock => {
                return Err(HubError::AlreadyRunning(paths.socket.display().to_string()));
            }
            std::fs::TryLockError::Error(e) => return Err(HubError::io("locking the hub lock", e)),
        }
    }

    private_file(&paths.database)?;
    let store = Arc::new(Store::open(&paths.database).map_err(HubError::OpenDatabase)?);
    let unfinished = recover(&store).map_err(HubError::Recover)?;

    let provider: Arc<dyn Provider> = Arc::new(LocalProvider::new(LocalConfig {
        worker_exe: config.worker_exe,
        instances_dir: paths.instances_dir.clone(),
        workspaces_root: paths.workspaces_root.clone(),
        shell: config.shell,
        path: config.path,
    }));
    for workspace in unfinished {
        warn!(workspace = workspace.name, "discarding a workspace left half-made by the last run");
        let _ = provider.destroy(&workspace.instance_id).await;
    }
    let hub = Arc::new(Hub {
        store,
        providers: HashMap::from([(LOCAL_PROVIDER.to_string(), provider)]),
        default_provider: LOCAL_PROVIDER.to_string(),
        workers: Mutex::new(HashMap::new()),
        worker_gates: Mutex::new(HashMap::new()),
        ensuring_session: AsyncMutex::new(()),
        sessions: Mutex::new(HashMap::new()),
        setup_gates: Mutex::new(HashMap::new()),
        feed: broadcast::channel(FEED_CAPACITY).0,
        started_at_ms: now_ms(),
        shutdown: Notify::new(),
        shutting_down: AtomicBool::new(false),
        name_salt: AtomicU64::new(now_ms().cast_unsigned()),
        next_run: AtomicU64::new(1),
        clients: AtomicU32::new(0),
    });

    // Holding the lock means any socket here is stale.
    let _ = std::fs::remove_file(&paths.socket);
    let listener = tokio::net::UnixListener::bind(&paths.socket)
        .map_err(|e| HubError::io(format!("binding {}", paths.socket.display()), e))?;
    // Only this user can connect, which is what authenticates a local client.
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&paths.socket, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| HubError::io("restricting the socket", e))?;
    }
    info!(socket = %paths.socket.display(), pid = std::process::id(), "hub listening");

    let accept = {
        let hub = hub.clone();
        tokio::spawn(server::accept_loop(hub, listener))
    };
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|e| HubError::io("listening for SIGTERM", e))?;
    tokio::select! {
        () = hub.shutdown.notified() => info!("shutdown requested"),
        _ = tokio::signal::ctrl_c() => info!("interrupted"),
        _ = terminate.recv() => info!("terminated"),
    }
    accept.abort();
    let _ = std::fs::remove_file(&paths.socket);
    hub.stop_all_workers().await;
    drop(lock_file);
    info!("hub stopped");
    Ok(())
}
