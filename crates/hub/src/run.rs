//! Starting a hub, and running it until it's asked to stop.

use std::path::PathBuf;
use std::sync::Arc;

use protocol::event::{EventBody, ExitReason};
use protocol::model::SessionStatus;
use protocol::paths::Paths;
use providers::Provider;
use providers::local::{LocalConfig, LocalProvider};
use tracing::{info, warn};

use crate::error::HubError;
use crate::hub::Hub;
use crate::listener::{self, Listener};
use crate::private;
use crate::provider_set::ProviderSet;
use crate::store::{RecordedInstance, SessionChange, StatusChange, Store, StoreError, cancelled_approval};
use crate::terminate::Terminate;

pub struct HubConfig {
    pub paths: Paths,
    /// The binary that runs `worker --stdio`.
    pub worker_exe: PathBuf,
    /// The user's shell, for finding the real `PATH`.
    pub shell: Option<String>,
    /// This process's own `PATH`.
    pub path: Option<String>,
    /// A client started this hub because it needed one.
    pub on_demand: bool,
}

/// Anything left at work or half-made by the last run. Its workers exited when it went away, and its agents too, so
/// a prompt one of them hadn't taken never will be. A queue whose agent went away at work is held, as when the agent
/// exits mid-turn with the hub still there. Returns the instances it left part way, to be destroyed once the hub is
/// serving, taken before anything new is made.
fn recover(store: &Store) -> Result<Vec<RecordedInstance>, StoreError> {
    store.fail_interrupted_setups()?;
    for (id, prompts) in store.untaken_prompts()? {
        for prompt in prompts {
            store.append(&id, None, &EventBody::PromptDropped { prompt }, None, None, None)?;
        }
    }
    for id in store.active_session_ids()? {
        for approval_id in store.open_approvals(&id)? {
            store.append(&id, None, &cancelled_approval(approval_id), None, None, None)?;
        }
        store.append(
            &id,
            None,
            &EventBody::AgentExited { reason: ExitReason::HubRestart, code: None, signal: None, stderr_tail: vec![] },
            None,
            None,
            // Its agent took what it was running in the background with it.
            Some(SessionChange {
                status: Some(StatusChange { background_tasks: Some(Vec::new()), ..SessionStatus::Interrupted.into() }),
                queue_held: Some(true),
            }),
        )?;
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
    Ok(ProviderSet::load(local, &paths.providers, &hub_id))
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
    let settings = store.hub_settings().map_err(HubError::OpenDatabase)?;

    let providers = load_providers(&paths, config.worker_exe, config.shell, config.path, &store)?;
    let hub = Arc::new(Hub::new(
        store,
        paths.attachments_dir.clone(),
        paths.worker_logs_dir.clone(),
        providers,
        config.on_demand,
    ));
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
    tokio::select! {
        () = hub.shutdown.notified() => info!("shutdown requested"),
        _ = tokio::signal::ctrl_c() => info!("interrupted"),
        () = terminate.asked() => info!("terminated"),
    }
    accept.abort();
    watching_pull_requests.abort();
    listener::unbind(&paths.socket);
    hub.stop_all_workers().await;
    hub.shut_down_providers().await;
    if let Some(awake) = awake {
        awake.abort();
    }
    drop(lock_file);
    info!("hub stopped");
    Ok(())
}

#[cfg(test)]
mod tests;
