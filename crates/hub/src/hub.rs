//! The hub's state, and what the rest of it shares: its workers, live sessions and feed.

use awake::Awake;
pub(crate) use carrying_on::LeftAtWork;
use setup::SetupGate;
pub(crate) use workspaces::workspace_env;

mod accounts;
mod agents;
mod attachments;
mod awake;
mod branches;
mod builds;
mod carrying_on;
mod children;
mod comments;
mod feed;
mod hub_settings;
mod instances;
mod logins;
mod new_workspace;
mod new_workspace_draft;
mod pull_requests;
mod queues;
mod ready;
mod repos;
mod sessions;
mod setup;
mod stopping;
mod terminals;
#[cfg(test)]
pub(crate) mod tests;
mod titles;
mod tools;
mod waits;
mod workers;
mod workspaces;
mod worktree;

use feed::Feed;
use sessions::Sessions;
#[cfg(test)]
pub(crate) use terminals::QUEUED_UPDATES;
use terminals::Terminals;
pub(crate) use terminals::{Numbered, Queued};

use crate::attachment_files::AttachmentFiles;
use crate::error::internal;
use crate::logins::Logins;
use crate::provider_set::ProviderSet;
use crate::session::{Item, LiveSession};
use crate::store::{MadeByRequest, Store, StoreError, now_ms};
use crate::versions;
use crate::worker_link::WorkerLink;
use parking_lot::Mutex;
use protocol::Build;
use protocol::client::{ApiError, ErrorCode, HubNotice, HubStatus};
use protocol::model::{AgentDescription, ClientRequestId, ProviderName, Records, SessionId, Workspace, WorkspaceId};
use providers::Provider;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, Notify, broadcast};
use tracing::warn;

/// Agents get 3 s to stop, then up to 5 s for each of their two streams to end, which something an agent left
/// running can hold open, and the worker exits within a second of flushing after that. So this is only reached if
/// something is stuck.
pub(super) const SHUTDOWN_GRACE: Duration = Duration::from_secs(15);
/// How long the hub waits for a provider as it starts, before it serves, to list its instances or say how to reach a
/// worker an earlier hub left running there. One that takes longer has its sessions interrupted, as if their workers had
/// gone, so a slow one doesn't keep every client waiting.
pub(crate) const BEFORE_SERVING: Duration = Duration::from_secs(5);

/// Whether a restart leaves the hub's workers running. On Windows the local provider's workers run on the hub's pipes,
/// since there are no Unix sockets there to serve them on, so they stop with it.
const KEEPS_WORKERS: bool = cfg!(unix);

/// What a worker is wanted for, which says whether it's stopped again after.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Wanted {
    /// Anything, such as a session, so it's kept running.
    ForAnything,
    /// Only a read nobody asked for, after which it's stopped, unless something else has wanted it meanwhile.
    OnlyToRead,
}

pub(crate) struct Hub {
    build: Build,
    store: Arc<Store>,
    attachment_files: AttachmentFiles,
    /// What each worker's command writes to stderr, a file per instance. Apart from any provider's own files.
    worker_logs_dir: PathBuf,
    logins: Arc<Logins>,
    providers: ProviderSet,
    /// What each kind of agent the hub runs is called and offers, as `HubConfig` gave them.
    agents: Vec<AgentDescription>,
    workers: Mutex<HashMap<WorkspaceId, Arc<WorkerLink>>>,
    /// Held while a workspace's worker starts or is cleaned up, so an instance never has two. One per
    /// workspace, because a slow or stuck worker mustn't hold up the others. It guards whether nothing but this hub's
    /// own worker can be on the instance, which is false until the hub has stopped whatever an earlier one left there.
    worker_gates: Mutex<HashMap<WorkspaceId, Arc<AsyncMutex<bool>>>>,
    /// Held to read while one of a workspace's other worktrees is made, and to write while one is removed, their
    /// records are settled or the workspace is archived, so nothing is removed or forgotten as it's made, which would
    /// leave it there unrecorded.
    child_gates: Mutex<HashMap<WorkspaceId, Arc<tokio::sync::RwLock<()>>>>,
    /// Held while finding or making a workspace's first session.
    ensuring_session: AsyncMutex<()>,
    sessions: Mutex<Sessions>,
    /// Workspaces whose setup is running. Removed as it finishes, which also wakes anyone waiting.
    setup_gates: Mutex<HashMap<WorkspaceId, SetupGate>>,
    terminals: Terminals,
    pull_requests: pull_requests::Reading,
    awake: Arc<Awake>,
    /// Whether the hub allows what an agent with full access still asks about, as its settings say.
    allows_asked_in_full_access: AtomicBool,
    /// Held while the hub's settings change.
    configuring: AsyncMutex<()>,
    feed: Feed,
    on_demand: bool,
    started_at_ms: i64,
    pub(crate) shutdown: Notify,
    shutting_down: AtomicBool,
    /// Asked to restart, which leaves its workers running for the next hub, rather than to stop.
    restarting: AtomicBool,
    /// Asked to stop, which stops its workers and their agents, even once it was asked to restart.
    stopping: AtomicBool,
    pub(crate) clients: AtomicU32,
    pub(crate) requests: AtomicU32,
}

/// Where in its data directory the hub keeps what it keeps outside the database.
pub(crate) struct HubDirs {
    pub(crate) attachments: PathBuf,
    pub(crate) worker_logs: PathBuf,
    pub(crate) logins: PathBuf,
}

impl HubDirs {
    /// Each in `dir`, for a test's hub.
    #[cfg(test)]
    pub(crate) fn in_dir(dir: &std::path::Path) -> Self {
        Self { attachments: dir.join("attachments"), worker_logs: dir.join("worker-logs"), logins: dir.join("logins") }
    }
}

impl Hub {
    pub(crate) fn new(
        build: Build,
        store: Arc<Store>,
        dirs: HubDirs,
        providers: ProviderSet,
        agents: Vec<AgentDescription>,
        on_demand: bool,
    ) -> Self {
        Self {
            build,
            logins: Arc::new(Logins::new(store.clone(), dirs.logins)),
            store,
            attachment_files: AttachmentFiles::new(dirs.attachments),
            worker_logs_dir: dirs.worker_logs,
            on_demand,
            providers,
            agents,
            workers: Mutex::new(HashMap::new()),
            worker_gates: Mutex::new(HashMap::new()),
            child_gates: Mutex::new(HashMap::new()),
            ensuring_session: AsyncMutex::new(()),
            sessions: Mutex::new(Sessions::default()),
            setup_gates: Mutex::new(HashMap::new()),
            terminals: Terminals::default(),
            pull_requests: pull_requests::Reading::new(),
            awake: Arc::default(),
            allows_asked_in_full_access: AtomicBool::new(false),
            configuring: AsyncMutex::new(()),
            feed: Feed::new(),
            started_at_ms: now_ms(),
            shutdown: Notify::new(),
            shutting_down: AtomicBool::new(false),
            restarting: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            clients: AtomicU32::new(0),
            requests: AtomicU32::new(0),
        }
    }

    /// Which build this hub is.
    pub(crate) fn build(&self) -> Build {
        self.build
    }

    /// The refusal of a request this hub doesn't know, or can't read.
    pub(crate) fn unsupported(&self, unreadable: Option<&serde_json::Error>) -> ApiError {
        ApiError::unsupported_request("this hub", self.build.version(), unreadable)
    }

    /// A workspace's provider. One the hub isn't set up with, such as one taken out of its settings, is unavailable
    /// until it's back.
    fn provider(&self, name: &ProviderName) -> Result<Arc<dyn Provider>, ApiError> {
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
        off_thread(move || f(&store)).await
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
        let (listed, terminals) = self.terminals.listed_with(self.blocking(Store::list_records)).await;
        Ok(Records { terminals, ..listed? })
    }

    pub(crate) async fn workspace(&self, id: &WorkspaceId) -> Result<Workspace, ApiError> {
        let id = id.clone();
        self.blocking(move |store| store.get_workspace(&id)).await
    }

    pub(crate) fn subscribe_feed(&self) -> broadcast::Receiver<HubNotice> {
        self.feed.subscribe()
    }

    /// Tells every client following the feed of `notice`, as [`Feed::send`] says.
    pub(crate) fn notify(&self, notice: HubNotice) {
        self.feed.send(notice);
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
        self.sessions.lock().keep(id, || LiveSession::start(self, stored, &approvals, None))
    }

    // Platform API --------------------------------------------------------------------------------

    pub(crate) fn status(&self) -> HubStatus {
        let running_sessions = self.sessions.lock().live.values().filter(|session| session.counts_as_running()).count();
        HubStatus {
            pid: std::process::id(),
            hub_version: self.build.version().to_string(),
            protocol_version: versions::CLIENTS.newest,
            started_at_ms: self.started_at_ms,
            workers: u32::try_from(self.workers.lock().len()).unwrap_or(u32::MAX),
            running_sessions: u32::try_from(running_sessions).unwrap_or(u32::MAX),
            clients: self.clients.load(Ordering::Relaxed),
            on_demand: self.on_demand,
            build_number: self.build.number(),
            running_setups: u32::try_from(self.setup_gates.lock().len()).unwrap_or(u32::MAX),
            requests: self.requests.load(Ordering::Relaxed),
            keeping_awake: self.awake.holding(),
            keeps_workers: KEEPS_WORKERS,
            oldest_protocol_version: Some(versions::CLIENTS.oldest),
            open_terminals: u32::try_from(self.terminals.count()).unwrap_or(u32::MAX),
        }
    }

    /// Says that what's at work may have changed, for whatever keeps this machine awake.
    pub(crate) fn work_changed(&self) {
        self.awake.work_changed();
    }
}

/// Runs `f` off the async threads, for what's synchronous, such as the store and the logins' files.
async fn off_thread<T, E>(f: impl FnOnce() -> Result<T, E> + Send + 'static) -> Result<T, ApiError>
where
    T: Send + 'static,
    E: Into<ApiError> + Send + 'static,
{
    tokio::task::spawn_blocking(f).await.map_err(|e| internal(e.to_string()))?.map_err(Into::into)
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
