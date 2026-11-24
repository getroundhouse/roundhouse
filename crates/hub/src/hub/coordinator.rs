use super::HubDirs;
use super::agents::ModelLists;
use super::followers::Followers;
use super::observations::{self, Observations};
use super::terminals::Terminals;
use super::updates::Updates;
use super::{awake, changes_watch, feed, github_observed, github_reads, pull_requests, sessions, setup};
use crate::attachment_files::AttachmentFiles;
use crate::cache_file::CacheFile;
use crate::error::internal;
use crate::gh::Gh;
use crate::github_cache::{self, GithubCache};
use crate::github_logins::GithubLogins;
use crate::jobs::Jobs;
use crate::logins::Logins;
use crate::machine_environment::MachineEnvironment;
use crate::provider_set::ProviderSet;
use crate::session::{Item, LiveSession, Starting};
use crate::ssh::Ssh;
use crate::store::{MadeByRequest, Store, StoreError, now_ms};
use crate::timers::Timers;
use crate::versions;
use crate::worker_link::WorkerLink;
use awake::Awake;
use feed::Feed;
use github::Github;
use parking_lot::Mutex;
use protocol::Build;
use protocol::client::{ApiError, ErrorCode, HubStart, HubStatus};
use protocol::model::{
    AgentDescription, AgentKind, ClientRequestId, GithubSignInId, Level, ProviderName, Records, SessionId, Workspace,
    WorkspaceId,
};
use providers::Provider;
use sessions::Sessions;
use setup::SetupGate;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use tokio::sync::{Mutex as AsyncMutex, Notify, watch};
use tracing::warn;

/// Whether a restart leaves the hub's workers running. On Windows the local provider's workers run on the hub's pipes,
/// since there are no Unix sockets there to serve them on, so they stop with it.
pub(super) const KEEPS_WORKERS: bool = cfg!(unix);

pub(crate) struct Hub {
    pub(super) build: Build,
    pub(super) store: Arc<Store>,
    pub(super) attachment_files: AttachmentFiles,
    /// Where the hub keeps archived git bundles for ephemeral workspaces whose clones are destroyed on archive.
    pub(super) bundles_dir: PathBuf,
    /// What each worker's command writes to stderr, a file per instance. Apart from any provider's own files.
    pub(super) worker_logs_dir: PathBuf,
    pub(super) logins: Arc<Logins>,
    pub(super) github_logins: Arc<GithubLogins>,
    /// What the hub asks GitHub, such as whose a token is.
    pub(super) github: Github,
    /// `gh` on the hub's own machine, whose login an account can use.
    pub(super) gh: Gh,
    /// `ssh` on the hub's own machine, whose key GitHub takes as someone's.
    pub(super) ssh: Ssh,
    /// What everything the hub starts on its machine is given, such as a worker's command.
    pub(crate) machine: MachineEnvironment,
    /// Told each time a worker has read a repository's address, whatever it read, for the check of reach to look again
    /// at which repository on GitHub each repository's record names.
    pub(super) addresses_read: Notify,
    pub(super) providers: ProviderSet,
    /// What each kind of agent the hub runs is called and offers, as `HubConfig` gave them.
    pub(super) agents: Vec<AgentDescription>,
    pub(super) workers: Mutex<HashMap<WorkspaceId, Arc<WorkerLink>>>,
    /// Held while a workspace's worker starts or is cleaned up, so an instance never has two. One per
    /// workspace, because a slow or stuck worker mustn't hold up the others. It guards whether nothing but the
    /// workspace's worker in [`Hub::workers`] can be on the instance: false until the hub has stopped whatever an
    /// earlier one left there, and from the moment a worker is started or let go of until it's in the map or the
    /// instance has been cleared, so a start or stop dropped partway leaves the next start to clear it. A start that
    /// fails leaves it false too, but one refused because another worker serves the instance leaves it as it was.
    pub(super) worker_gates: Mutex<HashMap<WorkspaceId, Arc<AsyncMutex<bool>>>>,
    /// Held to read while one of a workspace's other worktrees is made, and to write while one is removed, their
    /// records are settled or the workspace is archived, so nothing is removed or forgotten as it's made, which would
    /// leave it there unrecorded.
    pub(super) child_gates: Mutex<HashMap<WorkspaceId, Arc<tokio::sync::RwLock<()>>>>,
    /// Held while finding or making a workspace's first session.
    pub(super) ensuring_session: AsyncMutex<()>,
    /// The kind of agent each workspace being made from the new-workspace dialog is to run its first chat on, from the
    /// moment it's recorded until that chat is made, so a first chat asked for meanwhile, such as by a client showing
    /// the workspace once it's ready, is that one, whoever asks first.
    pub(super) first_chats: Mutex<HashMap<WorkspaceId, AgentKind>>,
    pub(super) sessions: Mutex<Sessions>,
    /// Workspaces whose setup is running. Removed as it finishes, which also wakes anyone waiting.
    pub(super) setup_gates: Mutex<HashMap<WorkspaceId, SetupGate>>,
    pub(super) terminals: Terminals,
    /// What the agents that discover their own models offer, where the hub has been asked.
    pub(super) model_lists: ModelLists,
    /// What the hub last found in the update feed, which it reads only as a service.
    pub(crate) updates: Updates,
    pub(super) pull_requests: pull_requests::Reading,
    /// What the hub last read from GitHub, for every part of it that reads GitHub, and every client.
    pub(super) github_cache: Arc<GithubCache>,
    /// The hub's observations of what others own, as it last observed them.
    pub(super) observations: Observations,
    /// The reads of workspaces' changes from their workers that are running now, by workspace, as `Hub::read_lines`
    /// says.
    pub(super) line_reads: Jobs<WorkspaceId>,
    /// What each workspace's worker was last told of watching its worktree.
    pub(super) changes_watches: changes_watch::Watches,
    /// Who follows what, which is what the hub keeps current.
    pub(super) followers: Followers,
    /// Tells of each read of a workspace's pull request that ends, for what's sent of it as observations.
    pub(super) github_reads_ended: github_observed::ReadsEnded,
    /// When the hub last tried to read each pull request of an archived workspace that the cache had forgotten.
    pub(super) github_forgotten: github_reads::Forgotten,
    /// The reads of workspaces' own pull requests nobody asked for that are running now, by whose token each reads
    /// as.
    pub(super) own_reads: Jobs<pull_requests::ReadsAs>,
    /// The sign-ins with GitHub the hub is asking GitHub after, a job for each, as `Hub::wait_for_sign_in` says.
    pub(super) sign_ins: Jobs<GithubSignInId>,
    /// Whether the hub has stopped checking its GitHub logins, as it does as it stops, so no check, and no renewal,
    /// starts after, as `Hub::stop_checking_github_logins` says.
    pub(super) github_checks_stopped: watch::Sender<bool>,
    pub(super) awake: Arc<Awake>,
    /// Whether the hub allows what an agent with full access still asks about, as its settings say.
    pub(super) allows_asked_in_full_access: AtomicBool,
    /// Held while the hub's settings change.
    pub(super) configuring: AsyncMutex<()>,
    pub(super) feed: Feed,
    pub(super) start: HubStart,
    pub(crate) timers: Timers,
    pub(super) started_at_ms: i64,
    pub(crate) shutdown: Notify,
    pub(super) shutting_down: AtomicBool,
    /// Asked to restart, which leaves its workers running for the next hub, rather than to stop.
    pub(super) restarting: AtomicBool,
    /// Asked to stop, which stops its workers and their agents, even once it was asked to restart.
    pub(super) stopping: AtomicBool,
    pub(crate) remote: crate::remote::Remote,
    pub(crate) clients: AtomicU32,
    pub(crate) requests: AtomicU32,
}

impl Hub {
    #[expect(
        clippy::too_many_arguments,
        reason = "each is a part of the hub made apart from it, handed over once, by `run` or a test's hub"
    )]
    pub(crate) fn new(
        build: Build,
        store: Arc<Store>,
        dirs: HubDirs,
        providers: ProviderSet,
        github: Github,
        machine: MachineEnvironment,
        agents: Vec<AgentDescription>,
        start: HubStart,
        timers: Timers,
    ) -> Self {
        let cache_file = Arc::new(CacheFile::open(&dirs.cache_file, &[&github_cache::PART, &observations::PART]));
        Self {
            remote: crate::remote::Remote::new(crate::remote::RemoteConfig {
                address: dirs.network_address,
                certificate: dirs.data.join("hub-cert.der"),
                certificate_key: dirs.data.join("hub-key.der"),
            }),
            build,
            logins: Arc::new(Logins::new(store.clone(), &dirs.data)),
            github_logins: Arc::new(GithubLogins::new(store.clone(), &dirs.data, now_ms())),
            github,
            ssh: Ssh::new(machine.clone()),
            gh: Gh::new(machine.clone(), dirs.data.clone()),
            machine,
            addresses_read: Notify::new(),
            observations: Observations::open(cache_file.clone(), &store),
            github_cache: Arc::new(GithubCache::open(cache_file)),
            line_reads: Jobs::new(),
            changes_watches: changes_watch::Watches::default(),
            store,
            attachment_files: AttachmentFiles::new(dirs.attachments),
            bundles_dir: dirs.bundles,
            worker_logs_dir: dirs.worker_logs,
            start,
            timers,
            providers,
            agents,
            workers: Mutex::new(HashMap::new()),
            worker_gates: Mutex::new(HashMap::new()),
            child_gates: Mutex::new(HashMap::new()),
            ensuring_session: AsyncMutex::new(()),
            first_chats: Mutex::new(HashMap::new()),
            sessions: Mutex::new(Sessions::default()),
            setup_gates: Mutex::new(HashMap::new()),
            terminals: Terminals::default(),
            model_lists: ModelLists::default(),
            updates: Updates::new(dirs.service_builds),
            pull_requests: pull_requests::Reading::new(),
            followers: Followers::default(),
            github_reads_ended: github_observed::ReadsEnded::default(),
            github_forgotten: github_reads::Forgotten::default(),
            own_reads: Jobs::new(),
            sign_ins: Jobs::new(),
            github_checks_stopped: watch::Sender::new(false),
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
    pub(super) fn provider(&self, name: &ProviderName) -> Result<Arc<dyn Provider>, ApiError> {
        self.providers
            .get(name)
            .ok_or_else(|| ApiError::new(ErrorCode::Unavailable, format!("there's no provider {name}")))
    }

    /// Whether Roundhouse leaves GitHub on `name`'s machines to the machine, as
    /// [`ProviderSet::leaves_github_to_machine`] says.
    pub(crate) fn leaves_github_to_machine(&self, name: &ProviderName) -> bool {
        self.providers.leaves_github_to_machine(name)
    }

    /// The builds of the worker for other machines than the hub's, as [`ProviderSet::workers`] says.
    pub(crate) fn worker_builds(&self) -> &[providers::WorkerBinary] {
        self.providers.workers()
    }

    /// Ends every job where it is, sign-ins', lookups of models and reads of workspaces' changes included, and starts
    /// none after, as the hub stops, before its workers are stopped or left. A sign-in left waiting is recorded as
    /// ended as the next hub starts, lists of models aren't kept, and a workspace's changes are read again as its
    /// worker says its branches to the next hub.
    pub(crate) fn stop_jobs(&self) {
        self.own_reads.stop();
        self.line_reads.stop();
        self.sign_ins.stop();
        self.model_lists.stop();
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

    /// Every record, and every observation the hub sends at its own level, as of one revision, for a client starting to
    /// follow the feed.
    pub(crate) async fn records_list(&self) -> Result<Records, ApiError> {
        let listing = self.model_lists.listed_with(self.blocking(Store::list_records));
        let listing = self.updates.listed_with(self.terminals.listed_with(listing));
        let ((((listed, model_lists), terminals), hub_update), observations) =
            self.observations.listed_with(&Level::Hub, listing).await;
        let mut records = Records { terminals, model_lists, hub_update, observations, ..listed? };
        // After the list, with what's kept then, which is never older than what the list's revision takes in. Newer is
        // only of a change whose record's revision moves after, so a client is sent the record again.
        self.observations.fill(&mut records);
        Ok(records)
    }

    pub(crate) async fn workspace(&self, id: &WorkspaceId) -> Result<Workspace, ApiError> {
        let id = id.clone();
        self.blocking(move |store| store.get_workspace(&id)).await
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
        let starting = self.blocking(move |store| Starting::read(store, &owned)).await?;
        self.sessions.lock().keep(id, || LiveSession::start(self, starting, None))
    }

    // Platform API --------------------------------------------------------------------------------

    pub(crate) fn status(&self) -> HubStatus {
        let running_sessions = self.sessions.lock().live.values().filter(|session| session.counts_as_running()).count();
        let (workers, oldest_running_worker_protocol) = self.workers_running();
        HubStatus {
            pid: std::process::id(),
            hub_version: self.build.version().to_string(),
            protocol_version: versions::CLIENTS.newest,
            started_at_ms: self.started_at_ms,
            workers,
            running_sessions: u32::try_from(running_sessions).unwrap_or(u32::MAX),
            clients: self.clients.load(Ordering::Relaxed),
            on_demand: self.start == HubStart::OnDemand,
            started_by: Some(self.start),
            build_number: self.build.number(),
            running_setups: u32::try_from(self.setup_gates.lock().len()).unwrap_or(u32::MAX),
            requests: self.requests.load(Ordering::Relaxed),
            keeping_awake: self.awake.holding(),
            keeps_workers: KEEPS_WORKERS,
            oldest_protocol_version: Some(versions::CLIENTS.oldest),
            oldest_running_worker_protocol: Some(oldest_running_worker_protocol),
            open_terminals: u32::try_from(self.terminals.count()).unwrap_or(u32::MAX),
        }
    }

    /// How many workers it has, and the oldest version of the worker protocol among those still open, counting its own,
    /// as [`HubStatus::oldest_running_worker_protocol`] says, from one look at them. A worker joins them once its hello
    /// has chosen its version, and a closed one is still among them until whatever ends it has given back its instance.
    pub(super) fn workers_running(&self) -> (u32, u32) {
        let workers = self.workers.lock();
        let oldest = workers
            .values()
            .filter(|link| link.is_open())
            .filter_map(|link| link.version())
            .fold(versions::WORKERS.newest, u32::min);
        (u32::try_from(workers.len()).unwrap_or(u32::MAX), oldest)
    }

    /// Says that what's at work may have changed, for whatever keeps this machine awake.
    pub(crate) fn work_changed(&self) {
        self.awake.work_changed();
    }
}

/// Runs `f` off the async threads, for what's synchronous, such as the store and the logins' files.
pub(super) async fn off_thread<T, E>(f: impl FnOnce() -> Result<T, E> + Send + 'static) -> Result<T, ApiError>
where
    T: Send + 'static,
    E: Into<ApiError> + Send + 'static,
{
    tokio::task::spawn_blocking(f).await.map_err(|e| internal(e.to_string()))?.map_err(Into::into)
}

pub(super) fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
