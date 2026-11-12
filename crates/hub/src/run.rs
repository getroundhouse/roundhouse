//! Starting a hub, and running it until it's asked to stop.

use std::path::PathBuf;
use std::sync::Arc;

use protocol::Build;
use protocol::model::AgentDescription;
use protocol::paths::Paths;
use providers::Provider;
use providers::local::{LocalConfig, LocalProvider};
use tracing::{info, warn};

use crate::error::HubError;
use crate::hub::{Hub, LeftAtWork};
use crate::listener::{self, Listener};
use crate::private;
use crate::provider_set::ProviderSet;
use crate::store::{RecordedInstance, Store, StoreError};
use crate::terminate::Terminate;

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

/// What the last run left half done: setups that were running, which its workers can't report on to this hub,
/// workspaces it was archiving or restoring, and workspaces and instances half-made. Returns the instances it left part
/// way, to be destroyed once the hub is serving, taken before anything new is made. What it left at work, the hub
/// carries on with as it starts.
fn recover(store: &Store) -> Result<Vec<RecordedInstance>, StoreError> {
    store.fail_interrupted_setups()?;
    // Before what it left at work is read, so the worker of one put back as ready is carried on with.
    for workspace in store.put_back_moves()? {
        let status = workspace.status;
        warn!(workspace = workspace.name, ?status, "putting back a workspace the last run was archiving or restoring");
    }
    for workspace in store.take_unfinished_workspaces()? {
        warn!(workspace = workspace.name, "discarding a workspace left half-made by the last run");
    }
    store.take_unfinished_instances()
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
    private::create_dir_all(&paths.data_dir)
        .map_err(|e| HubError::io(format!("creating {}", paths.data_dir.display()), e))?;
    let lock_file = take_lock(&paths)?;

    // Made here so it has its mode before SQLite opens it, which gives its -wal and -shm files the same one.
    private::file_options()
        .create(true)
        .append(true)
        .open(&paths.database)
        .map_err(|e| HubError::io(format!("creating {}", paths.database.display()), e))?;
    let store = Arc::new(Store::open(&paths.database, &paths.backups_dir).map_err(HubError::OpenDatabase)?);
    let unfinished = recover(&store).map_err(HubError::Recover)?;
    let left_at_work = LeftAtWork::read(&store).map_err(HubError::Recover)?;
    let settings = store.hub_settings().map_err(HubError::OpenDatabase)?;

    let providers = load_providers(config.build, &paths, config.worker_exe, config.shell, config.path, &store)?;
    let hub = Arc::new(Hub::new(
        config.build,
        store,
        paths.attachments_dir.clone(),
        paths.worker_logs_dir.clone(),
        providers,
        config.agents,
        config.on_demand,
    ));
    hub.carry_on(left_at_work).await;
    hub.carry_out(&settings);
    hub.tidy_draft_files().await;
    let awake = hub.keep_awake();
    hub.start_queues().await;

    // The lock is held, so nothing else is serving here.
    let listener =
        Listener::bind(&paths.socket).map_err(|e| HubError::io(format!("binding {}", paths.socket.display()), e))?;
    info!(socket = %paths.socket.display(), pid = std::process::id(), "hub listening");
    tokio::spawn(hub.clone().settle_providers(unfinished));

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
    drop(lock_file);
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

#[cfg(test)]
mod tests;
