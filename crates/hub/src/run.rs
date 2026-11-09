//! Starting a hub, and running it until it's asked to stop.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use protocol::event::{EventBody, ExitReason};
use protocol::model::{SessionStatus, Workspace};
use protocol::paths::Paths;
use providers::Provider;
use providers::local::{LocalConfig, LocalProvider};
use tracing::{info, warn};

use crate::error::HubError;
use crate::hub::Hub;
use crate::store::{Store, StoreError, cancelled_approval};

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

/// Anything left mid-turn or half-made by the last run. Its workers exited when it went away.
fn recover(store: &Store) -> Result<Vec<Workspace>, StoreError> {
    store.fail_interrupted_setups()?;
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
            Some(SessionStatus::Interrupted),
        )?;
    }
    store.take_unfinished_workspaces()
}

/// Only sets the mode on directories it creates. `ROUNDHOUSE_HOME` can point anywhere, and changing the
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

/// One hub at a time per data directory. It holds what's returned for as long as it runs.
fn take_lock(paths: &Paths) -> Result<std::fs::File, HubError> {
    let lock_file = std::fs::File::create(&paths.lock).map_err(|e| HubError::io("opening the hub lock", e))?;
    match lock_file.try_lock() {
        Ok(()) => Ok(lock_file),
        Err(std::fs::TryLockError::WouldBlock) => Err(HubError::AlreadyRunning(paths.socket.display().to_string())),
        Err(std::fs::TryLockError::Error(e)) => Err(HubError::io("locking the hub lock", e)),
    }
}

/// Only this user can connect, which is what authenticates a local client. The caller must hold
/// the lock, which means any socket already here is stale.
fn bind_socket(path: &Path) -> Result<tokio::net::UnixListener, HubError> {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::remove_file(path);
    let listener =
        tokio::net::UnixListener::bind(path).map_err(|e| HubError::io(format!("binding {}", path.display()), e))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| HubError::io("restricting the socket", e))?;
    Ok(listener)
}

/// Runs until asked to stop or signalled. Returns an error if another hub is using these paths.
pub async fn run(config: HubConfig) -> Result<(), HubError> {
    let paths = config.paths;
    private_dir(&paths.data_dir)?;
    let lock_file = take_lock(&paths)?;

    private_file(&paths.database)?;
    let store = Arc::new(Store::open(&paths.database, &paths.backups_dir).map_err(HubError::OpenDatabase)?);
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
    let hub = Arc::new(Hub::new(store, provider, config.on_demand));

    let listener = bind_socket(&paths.socket)?;
    info!(socket = %paths.socket.display(), pid = std::process::id(), "hub listening");

    let accept = {
        let hub = hub.clone();
        tokio::spawn(crate::server::accept_loop(hub, listener))
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
