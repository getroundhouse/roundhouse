//! The hub's state, and what the rest of it shares: its workers, live sessions and feed.

use setup::SetupGate;
pub(crate) use workspaces::workspace_env;

mod accounts;
mod attachments;
mod comments;
mod pull_requests;
mod repos;
mod sessions;
mod setup;
mod titles;
mod workspaces;
mod worktree;

use crate::attachment_files::AttachmentFiles;
use crate::error::internal;
use crate::session::{Item, LiveSession};
use crate::store::{MadeByRequest, Store, StoreError, now_ms};
use crate::worker_link::WorkerLink;
use parking_lot::Mutex;
use protocol::PROTOCOL_VERSION;
use protocol::client::{ApiError, ErrorCode, HubNotice, HubStatus, error_chain};
use protocol::model::{ClientRequestId, InstanceId, Records, SessionId, Workspace, WorkspaceId};
use protocol::worker::WorkerResponse;
use providers::Provider;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, Notify, broadcast, oneshot};
use tracing::warn;

const LOCAL_PROVIDER: &str = "local";
/// Status changes are rare, so a client only overflows this if it has stopped reading.
const FEED_CAPACITY: usize = 256;
/// Agents get 3 s to stop and the worker flushes after that, so this is only reached if something
/// is stuck.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(15);

/// What a worker is wanted for, which says whether it's stopped again after.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Wanted {
    /// Anything, such as a session, so it's kept running.
    ForAnything,
    /// Only a read nobody asked for, after which it's stopped, unless something else has wanted it meanwhile.
    OnlyToRead,
}

pub(crate) struct Hub {
    store: Arc<Store>,
    attachment_files: AttachmentFiles,
    providers: HashMap<String, Arc<dyn Provider>>,
    default_provider: String,
    workers: Mutex<HashMap<WorkspaceId, Arc<WorkerLink>>>,
    /// Held while a workspace's worker starts or is cleaned up, so an instance never has two. One per
    /// workspace, because a slow or stuck worker mustn't hold up the others.
    worker_gates: Mutex<HashMap<WorkspaceId, Arc<AsyncMutex<()>>>>,
    /// Held while finding or making a workspace's first session.
    ensuring_session: AsyncMutex<()>,
    sessions: Mutex<HashMap<SessionId, Arc<LiveSession>>>,
    /// Workspaces whose setup is running. Removed as it finishes, which also wakes anyone waiting.
    setup_gates: Mutex<HashMap<WorkspaceId, SetupGate>>,
    pull_requests: pull_requests::Reading,
    feed: broadcast::Sender<HubNotice>,
    on_demand: bool,
    started_at_ms: i64,
    pub(crate) shutdown: Notify,
    shutting_down: AtomicBool,
    name_salt: AtomicU64,
    pub(crate) clients: AtomicU32,
    pub(crate) requests: AtomicU32,
}

fn not_ready(workspace: &Workspace) -> ApiError {
    ApiError::new(ErrorCode::Conflict, format!("workspace {} is {:?}", workspace.name, workspace.status))
}

fn unexpected(reply: &WorkerResponse) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("unexpected reply from the worker: {reply:?}"))
}

impl Hub {
    pub(crate) fn new(
        store: Arc<Store>,
        attachments_dir: PathBuf,
        provider: Arc<dyn Provider>,
        on_demand: bool,
    ) -> Self {
        Self {
            store,
            attachment_files: AttachmentFiles::new(attachments_dir),
            on_demand,
            providers: HashMap::from([(LOCAL_PROVIDER.to_string(), provider)]),
            default_provider: LOCAL_PROVIDER.to_string(),
            workers: Mutex::new(HashMap::new()),
            worker_gates: Mutex::new(HashMap::new()),
            ensuring_session: AsyncMutex::new(()),
            sessions: Mutex::new(HashMap::new()),
            setup_gates: Mutex::new(HashMap::new()),
            pull_requests: pull_requests::Reading::new(),
            feed: broadcast::channel(FEED_CAPACITY).0,
            started_at_ms: now_ms(),
            shutdown: Notify::new(),
            shutting_down: AtomicBool::new(false),
            name_salt: AtomicU64::new(now_ms().cast_unsigned()),
            clients: AtomicU32::new(0),
            requests: AtomicU32::new(0),
        }
    }

    fn provider(&self, name: &str) -> Result<Arc<dyn Provider>, ApiError> {
        self.providers.get(name).cloned().ok_or_else(|| internal(format!("no provider {name}")))
    }

    pub(crate) async fn blocking<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&Store) -> Result<T, StoreError> + Send + 'static,
    {
        let store = self.store.clone();
        let result = tokio::task::spawn_blocking(move || f(&store)).await.map_err(|e| internal(e.to_string()))?;
        Ok(result?)
    }

    /// What a request with this ID made, the first time it was sent. Sent again, it gets that back, however
    /// things have changed since, rather than being checked again as if it were new. A comment deleted since
    /// is gone, along with its ID, so sending its request again adds it again.
    pub(crate) async fn made_earlier<T: MadeByRequest + Send + 'static>(
        &self,
        client_request_id: Option<&ClientRequestId>,
    ) -> Result<Option<T>, ApiError> {
        let Some(id) = client_request_id.cloned() else { return Ok(None) };
        self.blocking(move |store| store.made_by(&id)).await
    }

    /// Every record, as of one revision, for a client starting to follow the feed.
    pub(crate) async fn records_list(&self) -> Result<Records, ApiError> {
        self.blocking(Store::list_records).await
    }

    pub(crate) async fn workspace(&self, id: &WorkspaceId) -> Result<Workspace, ApiError> {
        let id = id.clone();
        self.blocking(move |store| store.get_workspace(&id)).await
    }

    pub(crate) fn subscribe_feed(&self) -> broadcast::Receiver<HubNotice> {
        self.feed.subscribe()
    }

    pub(crate) fn notify(&self, notice: HubNotice) {
        // An error only means nobody is listening.
        let _ = self.feed.send(notice);
    }

    fn worker_gate(&self, workspace_id: &WorkspaceId) -> Arc<AsyncMutex<()>> {
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
        let _starting = gate.lock().await;
        if let Some(link) = self.open_worker(workspace_id, wanted) {
            return Ok(link);
        }
        let provider = self.provider(&workspace.provider)?;
        let link = WorkerLink::open(self.clone(), provider, workspace.id.clone(), workspace.instance_id).await?;
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
        let _stopping = gate.lock().await;
        {
            let mut workers = self.workers.lock();
            let only_read =
                workers.get(&workspace.id).is_some_and(|current| Arc::ptr_eq(current, link) && !link.kept());
            if !only_read {
                return;
            }
            workers.remove(&workspace.id);
        }
        link.close();
        let stopped = match self.provider(&workspace.provider) {
            Ok(provider) => provider.stop(&workspace.instance_id).await.map_err(|e| error_chain(&e)),
            Err(e) => Err(e.message),
        };
        if let Err(e) = stopped {
            warn!(id = %workspace.id, "stopping a worker started to read a pull request: {e}");
        }
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
        let _cleaning_up = gate.lock().await;
        let replaced = {
            let mut workers = self.workers.lock();
            if workers.get(workspace_id).is_some_and(|current| Arc::ptr_eq(current, link)) {
                workers.remove(workspace_id);
            }
            // By a newer worker, which has the instance now. That happens once one that only a background read wanted
            // has been stopped, and another started before this one's output ended.
            workers.contains_key(workspace_id)
        };
        if !replaced && let Err(e) = provider.stop(instance_id).await {
            warn!(%instance_id, "stopping a worker failed: {}", error_chain(&e));
        }
        // Still under the gate, so these are queued before a replacement worker can start.
        let sessions: Vec<_> =
            self.sessions.lock().values().filter(|session| &session.workspace_id == workspace_id).cloned().collect();
        for session in sessions {
            session.send_item(Item::WorkerLost(link.id)).await;
        }
    }

    /// Output for a session goes through here on its way to the log.
    pub(crate) async fn route(self: &Arc<Self>, session_id: &SessionId, item: Item) {
        match self.live_session(session_id).await {
            Ok(session) => session.send_item(item).await,
            Err(e) => warn!(%session_id, "dropping output for an unknown session: {e}"),
        }
    }

    pub(crate) async fn live_session(self: &Arc<Self>, id: &SessionId) -> Result<Arc<LiveSession>, ApiError> {
        if let Some(session) = self.sessions.lock().get(id) {
            return Ok(session.clone());
        }
        let owned = id.clone();
        let (stored, approvals) = self
            .blocking(move |store| {
                let stored = store.get_session(&owned)?;
                let approvals = store.open_approvals(&owned)?;
                Ok((stored, approvals))
            })
            .await?;
        let mut sessions = self.sessions.lock();
        let session = sessions.entry(id.clone()).or_insert_with(|| LiveSession::start(self, stored, approvals));
        Ok(session.clone())
    }

    // Platform API --------------------------------------------------------------------------------

    pub(crate) fn request_shutdown(&self) {
        self.shutdown.notify_one();
    }

    pub(crate) fn status(&self) -> HubStatus {
        let running_sessions = self.sessions.lock().values().filter(|session| session.status().is_active()).count();
        HubStatus {
            pid: std::process::id(),
            hub_version: protocol::BUILD_VERSION.to_string(),
            protocol_version: PROTOCOL_VERSION,
            started_at_ms: self.started_at_ms,
            workers: u32::try_from(self.workers.lock().len()).unwrap_or(u32::MAX),
            running_sessions: u32::try_from(running_sessions).unwrap_or(u32::MAX),
            clients: self.clients.load(Ordering::Relaxed),
            on_demand: self.on_demand,
            build_number: protocol::BUILD_NUMBER,
            running_setups: u32::try_from(self.setup_gates.lock().len()).unwrap_or(u32::MAX),
            requests: self.requests.load(Ordering::Relaxed),
        }
    }

    /// Stops every worker and waits until what they said is recorded.
    pub(crate) async fn stop_all_workers(&self) {
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
async fn close_worker(link: &WorkerLink, provider: &dyn Provider, instance_id: &InstanceId) {
    link.close();
    if tokio::time::timeout(SHUTDOWN_GRACE, link.wait_closed()).await.is_err() {
        warn!(%instance_id, "a worker didn't stop in time");
        if let Err(e) = provider.stop(instance_id).await {
            warn!(%instance_id, "stopping a stuck worker failed: {}", error_chain(&e));
        }
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
