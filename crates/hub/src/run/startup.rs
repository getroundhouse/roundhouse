use crate::error::HubError;
use crate::hub::{Hub, HubDirs, LeftAtWork};
use crate::listener;
use crate::listener::Listener;
use crate::provider_set::ProviderSet;
use crate::store::{Move, RecordedInstance, Store, StoreError};
use crate::terminate::Terminate;
use protocol::Build;
use protocol::model::{AgentDescription, WorkspaceId};
use protocol::paths::Paths;
use providers::Provider;
use providers::local::{LocalConfig, LocalProvider};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

pub struct HubConfig {
    /// Which build this hub is, which it tells clients, workers and provider adapters.
    pub build: Build,
    pub paths: Paths,
    /// The binary that runs a worker, which the local provider starts.
    pub worker_exe: PathBuf,
    /// The user's shell, for finding the real `PATH`.
    pub shell: Option<String>,
    /// This process's own `PATH`.
    pub path: Option<String>,
    /// A client started this hub because it needed one.
    pub on_demand: bool,
    /// What each kind of agent this hub runs is called and offers, as its adapter describes it. A kind with none is one
    /// the hub doesn't run.
    pub agents: Vec<AgentDescription>,
}

/// What the last run left half done, for this one to finish once it's serving.
pub(super) struct LeftHalfDone {
    /// The instances it left part way, to be destroyed, taken before anything new is made.
    pub(super) instances: Vec<RecordedInstance>,
    /// The workspaces it was archiving or restoring, which stay recorded as that until each is settled.
    pub(super) moves: Vec<(WorkspaceId, Move)>,
}

/// Clears up what the last run left half done: setups that were running, which its workers can't report on to this hub,
/// and workspaces half-made. It forgets which build each workspace's worker was, too, since only a worker this hub
/// carries on with can be another build than its own. Returns what's left for the hub to finish once it's serving. What
/// it left at work, the hub carries on with as it starts.
pub(super) fn recover(store: &Store) -> Result<LeftHalfDone, StoreError> {
    store.fail_interrupted_setups()?;
    store.forget_worker_builds()?;
    let moves = store.moves_left()?;
    for workspace in store.take_unfinished_workspaces()? {
        warn!(workspace = workspace.name, "discarding a workspace left half-made by the last run");
    }
    Ok(LeftHalfDone { instances: store.take_unfinished_instances()?, moves })
}

/// The hub's lock, once the store it keeps to this hub is open. Letting go of it stops the store writing first, for the
/// rest of this process: what this hub started can still be running until the daemon's runtime lets it go, and a new
/// hub can take the lock as soon as it's free, so a write from then on would land under the new hub as it copies the
/// database and migrates it. Letting go waits for a write under way, on whatever thread drops this, which in `run` is
/// the one `block_on` runs it on, not one of the runtime's.
pub(super) struct HubLock {
    pub(super) store: Arc<Store>,
    /// Let go of once [`Drop::drop`] has run, as a field is.
    pub(super) _lock: std::fs::File,
}

impl Drop for HubLock {
    fn drop(&mut self) {
        self.store.stop_writing();
    }
}

/// One hub at a time per data directory. It holds what's returned for as long as it runs.
fn take_lock(paths: &Paths) -> Result<std::fs::File, HubError> {
    let lock_file = std::fs::File::create(&paths.lock).map_err(|e| HubError::io("opening the hub lock", e))?;
    match lock_file.try_lock() {
        Ok(()) => Ok(lock_file),
        Err(std::fs::TryLockError::WouldBlock) => Err(HubError::AlreadyRunning(paths.socket.display().to_string())),
        Err(std::fs::TryLockError::Error(e)) => Err(HubError::io("locking the hub lock", e)),
    }
}

/// The local provider, and the others this hub's data directory names.
fn load_providers(
    build: Build,
    paths: &Paths,
    worker_exe: PathBuf,
    shell: Option<String>,
    path: Option<String>,
    store: &Store,
) -> Result<ProviderSet, HubError> {
    let local: Arc<dyn Provider> = Arc::new(LocalProvider::new(LocalConfig {
        worker_exe,
        instances_dir: paths.instances_dir.clone(),
        workspaces_root: paths.workspaces_root.clone(),
        shell,
        path,
    }));
    let hub_id = store.hub_id().map_err(HubError::OpenDatabase)?;
    Ok(ProviderSet::load(local, &paths.providers, &hub_id, build))
}

/// Runs until asked to stop or signalled. Returns an error if another hub is using these paths.
pub async fn run(config: HubConfig) -> Result<(), HubError> {
    let paths = config.paths;
    private_files::create_dir_all(&paths.data_dir)
        .map_err(|e| HubError::io(format!("creating {}", paths.data_dir.display()), e))?;
    let lock_file = take_lock(&paths)?;

    // Made here so it has its mode before SQLite opens it, which gives its -wal and -shm files the same one.
    private_files::file_options()
        .create(true)
        .append(true)
        .open(&paths.database)
        .map_err(|e| HubError::io(format!("creating {}", paths.database.display()), e))?;
    let store = Arc::new(Store::open(&paths.database, &paths.backups_dir).map_err(HubError::OpenDatabase)?);
    let lock = HubLock { store: store.clone(), _lock: lock_file };
    let unfinished = recover(&store).map_err(HubError::Recover)?;
    let left_at_work = LeftAtWork::read(&store).map_err(HubError::Recover)?;
    let settings = store.hub_settings().map_err(HubError::OpenDatabase)?;

    let providers = load_providers(config.build, &paths, config.worker_exe, config.shell, config.path, &store)?;
    let dirs = HubDirs {
        attachments: paths.attachments_dir.clone(),
        worker_logs: paths.worker_logs_dir.clone(),
        logins: paths.logins_dir.clone(),
    };
    let hub = Arc::new(Hub::new(config.build, store, dirs, providers, config.agents, config.on_demand));
    // Before clients are served, so none is told the hub holds a login whose token is gone.
    hub.tidy_logins().await;
    // Before carrying on with the agents an earlier hub left, since what they asked meanwhile comes in as soon as
    // their workers are connected again, and is answered as the settings say.
    hub.carry_out(&settings);
    hub.carry_on(left_at_work).await;
    hub.tidy_draft_files().await;
    let awake = hub.keep_awake();
    hub.start_queues().await;

    // The lock is held, so nothing else is serving here.
    let listener =
        Listener::bind(&paths.socket).map_err(|e| HubError::io(format!("binding {}", paths.socket.display()), e))?;
    info!(socket = %paths.socket.display(), pid = std::process::id(), "hub listening");
    tokio::spawn(hub.clone().settle_providers(unfinished.instances));
    tokio::spawn(hub.clone().settle_moves(unfinished.moves));

    let accept = {
        let hub = hub.clone();
        tokio::spawn(crate::server::accept_loop(hub, listener))
    };
    let watching_pull_requests = tokio::spawn(hub.clone().watch_pull_requests());
    let mut terminate = Terminate::listen().map_err(|e| HubError::io("listening for a signal to stop", e))?;
    let signalled = asked_to_go(&hub, &mut terminate).await;
    accept.abort();
    watching_pull_requests.abort();
    listener::unbind(&paths.socket);
    if hub.restarting() && !signalled {
        hub.leave_workers().await;
    } else {
        hub.stop_all_workers().await;
    }
    hub.shut_down_providers().await;
    if let Some(awake) = awake {
        awake.abort();
    }
    drop(lock);
    info!("hub stopped");
    Ok(())
}

/// Waits until the hub is asked to go, and says whether a signal asked. Whoever sends one, such as systemd, can't say
/// whether another hub is coming, so a signal stops the agents, even once the hub was asked to restart.
async fn asked_to_go(hub: &Hub, terminate: &mut Terminate) -> bool {
    tokio::select! {
        () = hub.shutdown.notified() => {
            info!("shutdown requested");
            false
        }
        _ = tokio::signal::ctrl_c() => {
            info!("interrupted");
            true
        }
        () = terminate.asked() => {
            info!("terminated");
            true
        }
    }
}
