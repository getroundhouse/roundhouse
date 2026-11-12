//! The hub's state, and what the rest of it shares: its workers, live sessions and feed.

use awake::Awake;
use setup::SetupGate;
pub(crate) use workspaces::workspace_env;

mod accounts;
mod attachments;
mod awake;
mod comments;
mod hub_settings;
mod instances;
mod pull_requests;
mod queues;
mod repos;
mod sessions;
mod setup;
mod titles;
mod workers;
mod workspaces;
mod worktree;

use sessions::Sessions;

use crate::attachment_files::AttachmentFiles;
use crate::error::internal;
use crate::provider_set::ProviderSet;
use crate::session::{Item, LiveSession};
use crate::store::{MadeByRequest, Store, StoreError, now_ms};
use crate::worker_link::WorkerLink;
use parking_lot::Mutex;
use protocol::client::CLIENT_PROTOCOL_VERSION;
use protocol::client::{ApiError, ErrorCode, HubNotice, HubStatus};
use protocol::model::{ClientRequestId, Records, SessionId, Workspace, WorkspaceId};
use protocol::worker::WorkerResponse;
use providers::Provider;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, Notify, broadcast, oneshot};
use tokio::task::JoinSet;
use tracing::warn;

/// Status changes are rare, so a client only overflows this if it has stopped reading.
const FEED_CAPACITY: usize = 256;
/// Agents get 3 s to stop, then up to 5 s for each of their two streams to end, which something an agent left
/// running can hold open, and the worker exits within a second of flushing after that. So this is only reached if
/// something is stuck.
pub(super) const SHUTDOWN_GRACE: Duration = Duration::from_secs(15);

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
    /// What each worker's command writes to stderr, a file per instance. Apart from any provider's own files.
    worker_logs_dir: PathBuf,
    providers: ProviderSet,
    workers: Mutex<HashMap<WorkspaceId, Arc<WorkerLink>>>,
    /// Held while a workspace's worker starts or is cleaned up, so an instance never has two. One per
    /// workspace, because a slow or stuck worker mustn't hold up the others. It guards whether nothing but this hub's
    /// own worker can be on the instance, which is false until the hub has stopped whatever an earlier one left there.
    worker_gates: Mutex<HashMap<WorkspaceId, Arc<AsyncMutex<bool>>>>,
    /// Held while finding or making a workspace's first session.
    ensuring_session: AsyncMutex<()>,
    sessions: Mutex<Sessions>,
    /// Workspaces whose setup is running. Removed as it finishes, which also wakes anyone waiting.
    setup_gates: Mutex<HashMap<WorkspaceId, SetupGate>>,
    pull_requests: pull_requests::Reading,
    awake: Arc<Awake>,
    /// Held while the hub's settings change.
    configuring: AsyncMutex<()>,
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
        worker_logs_dir: PathBuf,
        providers: ProviderSet,
        on_demand: bool,
    ) -> Self {
        Self {
            store,
            attachment_files: AttachmentFiles::new(attachments_dir),
            worker_logs_dir,
            on_demand,
            providers,
            workers: Mutex::new(HashMap::new()),
            worker_gates: Mutex::new(HashMap::new()),
            ensuring_session: AsyncMutex::new(()),
            sessions: Mutex::new(Sessions::default()),
            setup_gates: Mutex::new(HashMap::new()),
            pull_requests: pull_requests::Reading::new(),
            awake: Arc::default(),
            configuring: AsyncMutex::new(()),
            feed: broadcast::channel(FEED_CAPACITY).0,
            started_at_ms: now_ms(),
            shutdown: Notify::new(),
            shutting_down: AtomicBool::new(false),
            name_salt: AtomicU64::new(now_ms().cast_unsigned()),
            clients: AtomicU32::new(0),
            requests: AtomicU32::new(0),
        }
    }

    /// A workspace's provider. One the hub isn't set up with, such as one taken out of its settings, is unavailable
    /// until it's back.
    fn provider(&self, name: &str) -> Result<Arc<dyn Provider>, ApiError> {
        self.providers
            .get(name)
            .ok_or_else(|| ApiError::new(ErrorCode::Unavailable, format!("there's no provider {name}")))
    }

    /// Ends each provider's adapter, after the workers are stopped.
    pub(crate) async fn shut_down_providers(&self) {
        self.providers.shut_down().await;
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

    /// Output for a session goes through here on its way to the log.
    pub(crate) async fn route(self: &Arc<Self>, session_id: &SessionId, item: Item) {
        match self.live_session(session_id).await {
            Ok(session) => session.send_item(item).await,
            Err(e) => warn!(%session_id, "dropping output for an unknown session: {e}"),
        }
    }

    pub(crate) async fn live_session(self: &Arc<Self>, id: &SessionId) -> Result<Arc<LiveSession>, ApiError> {
        if let Some(session) = self.sessions.lock().live.get(id) {
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
        self.sessions.lock().keep(id, || LiveSession::start(self, stored, approvals))
    }

    // Platform API --------------------------------------------------------------------------------

    pub(crate) fn request_shutdown(&self) {
        self.shutdown.notify_one();
    }

    /// Stops if nothing would notice, as the one asking sees it, and refuses otherwise. What that client sent before
    /// this either still counts as a request or has left its mark, such as an agent at work, since a prompt's agent
    /// is at work from when the prompt is recorded.
    pub(crate) fn shutdown_if_idle(&self) -> Result<(), ApiError> {
        if !self.status().is_idle() {
            return Err(ApiError::new(ErrorCode::Conflict, "the hub is busy, so it wasn't stopped"));
        }
        self.request_shutdown();
        Ok(())
    }

    pub(crate) fn status(&self) -> HubStatus {
        let running_sessions =
            self.sessions.lock().live.values().filter(|session| session.status().is_active()).count();
        HubStatus {
            pid: std::process::id(),
            hub_version: protocol::BUILD_VERSION.to_string(),
            protocol_version: CLIENT_PROTOCOL_VERSION,
            started_at_ms: self.started_at_ms,
            workers: u32::try_from(self.workers.lock().len()).unwrap_or(u32::MAX),
            running_sessions: u32::try_from(running_sessions).unwrap_or(u32::MAX),
            clients: self.clients.load(Ordering::Relaxed),
            on_demand: self.on_demand,
            build_number: protocol::BUILD_NUMBER,
            running_setups: u32::try_from(self.setup_gates.lock().len()).unwrap_or(u32::MAX),
            requests: self.requests.load(Ordering::Relaxed),
            keeping_awake: self.awake.holding(),
        }
    }

    /// Says that what's at work may have changed, for whatever keeps this machine awake.
    pub(crate) fn work_changed(&self) {
        self.awake.work_changed();
    }

    /// Stops every worker and waits until what they said is recorded.
    pub(crate) async fn stop_all_workers(self: &Arc<Self>) {
        // Refuse new workers first, so a client still connected can't start one behind this.
        self.shutting_down.store(true, Ordering::SeqCst);
        let links: Vec<_> = self.workers.lock().iter().map(|(id, link)| (id.clone(), link.clone())).collect();
        for (_, link) in &links {
            link.close();
        }
        // One deadline for all of them, so several stuck workers don't add up.
        let deadline = tokio::time::Instant::now() + SHUTDOWN_GRACE;
        let mut stuck = JoinSet::new();
        for (workspace_id, link) in links {
            if tokio::time::timeout_at(deadline, link.wait_closed()).await.is_err() {
                warn!(%workspace_id, "a worker didn't stop in time");
                // The hub's exit would end the worker, but not its agents, which would run on unwatched.
                let hub = self.clone();
                stuck.spawn(async move { hub.end_stuck_worker_of(&workspace_id, &link).await });
            }
        }
        stuck.join_all().await;
        let sessions: Vec<_> = self.sessions.lock().live.values().cloned().collect();
        for session in sessions {
            let (done, flushed) = oneshot::channel();
            session.send_item(Item::Flush(done)).await;
            let _ = flushed.await;
        }
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
