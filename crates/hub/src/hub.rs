//! The hub's state, and what the rest of it shares: its workers, live sessions and feed.

pub(crate) use workspaces::workspace_env;

mod comments;
mod repos;
mod sessions;
mod setup;
mod workspaces;

use crate::error::internal;
use crate::session::{Item, LiveSession};
use crate::store::{Store, StoreError, now_ms};
use crate::worker_link::WorkerLink;
use parking_lot::Mutex;
use protocol::PROTOCOL_VERSION;
use protocol::client::{ApiError, ErrorCode, HubNotice, HubStatus, error_chain};
use protocol::model::{InstanceId, Records, SessionId, SessionStatus, Workspace, WorkspaceId};
use protocol::worker::WorkerResponse;
use providers::Provider;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, Notify, broadcast, oneshot, watch};
use tracing::warn;

const LOCAL_PROVIDER: &str = "local";
/// Status changes are rare, so a client only overflows this if it has stopped reading.
const FEED_CAPACITY: usize = 256;
/// Agents get 3 s to stop and the worker flushes after that, so this is only reached if something
/// is stuck.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(15);

pub(crate) struct Hub {
    store: Arc<Store>,
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
    setup_gates: Mutex<HashMap<WorkspaceId, watch::Sender<bool>>>,
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
    pub(crate) fn new(store: Arc<Store>, provider: Arc<dyn Provider>, on_demand: bool) -> Self {
        Self {
            store,
            on_demand,
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

    /// The running worker, if there is one.
    pub(crate) fn existing_worker(&self, workspace_id: &WorkspaceId) -> Option<Arc<WorkerLink>> {
        self.workers.lock().get(workspace_id).filter(|link| link.is_open()).cloned()
    }

    /// The workspace's worker, starting it if needed.
    pub(crate) async fn ensure_worker(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
    ) -> Result<Arc<WorkerLink>, ApiError> {
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
        workspace_id: &WorkspaceId,
        link: &Arc<WorkerLink>,
        provider: &dyn Provider,
        instance_id: &InstanceId,
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
        let running_sessions = self
            .sessions
            .lock()
            .values()
            .filter(|session| matches!(session.status(), SessionStatus::Running | SessionStatus::WaitingApproval))
            .count();
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
