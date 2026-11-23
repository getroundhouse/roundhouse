use crate::error::HubError;
use crate::hub::{Hub, HubDirs, LeftAtWork, UpdateFeed};
use crate::listener;
use crate::listener::Listener;
use crate::machine_environment::{LoginShell, MachineEnvironment};
use crate::provider_set::ProviderSet;
use crate::store::{Move, RecordedInstance, Store, StoreError};
use crate::terminate::Terminate;
use crate::timers::Timers;
use github::{Github, GithubConfig};
use process::Environment;
use protocol::Build;
use protocol::client::HubStart;
use protocol::model::{AgentDescription, WorkspaceId};
use protocol::paths::Paths;
use providers::Provider;
use providers::local::{LocalConfig, LocalProvider};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::task::JoinSet;
use tracing::{info, warn};

pub struct HubConfig {
    /// Which build this hub is, which it tells clients, workers and provider adapters.
    pub build: Build,
    /// Where paired devices can reach this hub, once pairing is enabled.
    pub network_address: std::net::SocketAddr,
    pub paths: Paths,
    /// The binary that runs a worker, which the local provider starts, and which the login shell runs to print the
    /// environment the profile gives, as [`process::PRINTS_ENVIRONMENT`] asks.
    pub worker_exe: PathBuf,
    /// The user's login shell, which the hub asks for the environment their profile gives, or `None` for none to ask.
    pub shell: Option<String>,
    /// The user's home, where the login shell starts.
    pub home: Option<PathBuf>,
    /// This process's own `PATH`.
    pub path: Option<String>,
    /// What started this hub, which its status says, so a client knows whether it may restart it.
    pub start: HubStart,
    /// What each kind of agent this hub runs is called and offers, as its adapter describes it. A kind with none is one
    /// the hub doesn't run.
    pub agents: Vec<AgentDescription>,
    pub timers: Timers,
    /// Where a hub installed as a service reads the update feed, for the release it would update to. `None` for any
    /// other hub, which updates as whoever runs it says.
    pub update_feed: Option<UpdateFeed>,
    /// Where the hub calls GitHub, and as which app it signs people in, as [`GithubConfig::from_env`] gives it. Given
    /// rather than known here, so a hub can be pointed at a stand-in, as the hub's own tests point theirs.
    pub github: GithubConfig,
}

/// What the last run left half done, for this one to finish once it's serving.
pub(super) struct LeftHalfDone {
    /// The instances it left part way, to be destroyed, taken before anything new is made.
    pub(super) instances: Vec<RecordedInstance>,
    /// The workspaces it was archiving or restoring, which stay recorded as that until each is settled.
    pub(super) moves: Vec<(WorkspaceId, Move)>,
}

/// Clears up what the last run left half done: setups that were running, which its workers can't report on to this hub,
/// sign-ins with GitHub left waiting, which only it could ask GitHub after, and workspaces half-made. It forgets which
/// build each workspace's worker was, too, since only a worker this hub carries on with can be another build than its
/// own, and the GitHub login found on each provider's machines, since each is asked again. Returns what's left for the
/// hub to finish once it's serving. What it left at work, the hub carries on with as it starts.
pub(super) fn recover(store: &Store) -> Result<LeftHalfDone, StoreError> {
    store.fail_interrupted_setups()?;
    store.end_waiting_github_sign_ins()?;
    store.forget_worker_builds()?;
    store.github_reach_left_by_the_last_run()?;
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

/// The local provider, and the others this hub's data directory names, whose adapters have as long to say hello as
/// `timers` says.
fn load_providers(
    build: Build,
    paths: &Paths,
    worker_exe: PathBuf,
    machine: &MachineEnvironment,
    store: &Store,
    timers: &Timers,
) -> Result<ProviderSet, HubError> {
    let local: Arc<dyn Provider> = Arc::new(LocalProvider::new(LocalConfig {
        worker_exe,
        instances_dir: paths.instances_dir.clone(),
        workspaces_root: paths.workspaces_root.clone(),
        clones_dir: paths.clones_dir.clone(),
    }));
    let hub_id = store.hub_id().map_err(HubError::OpenDatabase)?;
    let hello_within = timers.adapter_hello_within;
    let providers = ProviderSet::load(local, &paths.providers, &hub_id, build, hello_within, machine);
    // Before any client reads the settings, so none is told of the providers an earlier run had.
    let (left, set_up) = (providers.left_to_machine(), providers.set_up_github());
    store.record_github_providers(&left, &set_up).map_err(HubError::OpenDatabase)?;
    Ok(providers)
}

/// How a hub's run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    /// It stopped its workers, and their agents with them.
    Stopped,
    /// It left its workers running for the next hub, which it was asked to restart for, and which whatever started
    /// this one starts.
    Restarting,
}

/// Runs until asked to stop or signalled, and says whether another hub is to start. Returns an error if another hub
/// is using these paths.
pub async fn run(config: HubConfig) -> Result<Ended, HubError> {
    let update_feed = config.update_feed;
    let HubConfig {
        network_address, build, paths, worker_exe, shell, home, path, start, agents, timers, github, ..
    } = config;
    private_files::create_dir_all(&paths.data_dir)
        .map_err(|e| HubError::io(format!("creating {}", paths.data_dir.display()), e))?;
    let lock_file = take_lock(&paths)?;

    let store = open_store(&paths)?;
    let lock = HubLock { store: store.clone(), _lock: lock_file };
    let unfinished = recover(&store).map_err(HubError::Recover)?;
    let left_at_work = LeftAtWork::read(&store).map_err(HubError::Recover)?;
    let settings = store.hub_settings().map_err(HubError::OpenDatabase)?;

    // Before anything the hub starts, a provider's adapter or a worker it carries on with, so none waits on a slow
    // profile within its own time limit.
    let machine = read_machine_environment(shell.as_deref(), home.as_deref(), &worker_exe, path.as_deref()).await;
    let providers = load_providers(build, &paths, worker_exe, &machine, &store, &timers)?;
    let github = Github::new(github);
    let dirs = HubDirs { network_address, ..HubDirs::of(&paths) };
    let hub = Arc::new(Hub::new(build, store, dirs, providers, github, machine, agents, start, timers));
    // Before any client is served, so none is told the hub doesn't read the feed while it's starting to.
    read_update_feed(&hub, update_feed, build);
    // Before clients are served, so none is told the hub holds a login whose token is gone.
    hub.tidy_logins().await;
    // Before carrying on with the agents an earlier hub left, since what they asked meanwhile comes in as soon as
    // their workers are connected again, and is answered as the settings say.
    hub.carry_out(&settings);
    hub.call_off_unwatched_waits().await;
    hub.carry_on(left_at_work).await;
    hub.tidy_draft_files().await;
    let awake = hub.keep_awake();
    hub.start_queues().await;
    // What the hub does by itself while it serves, each ended where it is as it stops, since what any would do is done
    // again as the next hub starts. Telling of what workers are sent comes before any client is served, here or paired,
    // so each hears every change after the records it lists.
    let mut serving = JoinSet::new();
    serving.spawn(hub.tell_of_what_workers_are_sent());
    hub.listen_if_paired().await;

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
    watch_while_serving(&hub, &mut serving);
    let watching_github_logins = tokio::spawn(hub.clone().watch_github_logins());
    let mut terminate = Terminate::listen().map_err(|e| HubError::io("listening for a signal to stop", e))?;
    let signalled = asked_to_go(&hub, &mut terminate).await;
    accept.abort();
    hub.remote.stop_listening().await;
    serving.abort_all();
    hub.stop_jobs();
    listener::unbind(&paths.socket);
    // Before the lock goes, so what a renewal under way is given is written before the next hub can start.
    hub.stop_checking_github_logins(watching_github_logins).await;
    let ended = end_workers(&hub, signalled).await;
    hub.shut_down_providers().await;
    if let Some(awake) = awake {
        awake.abort();
    }
    drop(lock);
    info!("hub stopped");
    Ok(ended)
}

/// What everything the hub starts on its machine is given, as `shell`, started in `home`, has it once it has read the
/// profile, with `printer`, the daemon, printing it, over the hub's own environment, whose `PATH` is `path`.
async fn read_machine_environment(
    shell: Option<&str>,
    home: Option<&Path>,
    printer: &Path,
    path: Option<&str>,
) -> MachineEnvironment {
    // Nothing beyond the hub's own environment, which the shell inherits.
    let given = Environment::NONE;
    let login = shell.map(|shell| LoginShell { shell, home, given: &given });
    MachineEnvironment::read(login.as_ref(), printer, path).await
}

/// Leaves `hub`'s workers running for the next hub, if it was asked to restart and no signal asked it to go, and stops
/// them otherwise, with their agents.
async fn end_workers(hub: &Arc<Hub>, signalled: bool) -> Ended {
    if hub.restarting() && !signalled {
        hub.leave_workers().await;
        Ended::Restarting
    } else {
        hub.stop_all_workers().await;
        Ended::Stopped
    }
}

/// What `hub` watches while it serves, each a task of `serving`: its waits' deadlines, its pull requests, what's sent
/// of GitHub as observations, its reach on GitHub, and the update feed, if it reads one.
fn watch_while_serving(hub: &Arc<Hub>, serving: &mut JoinSet<()>) {
    serving.spawn(Arc::clone(hub).watch_wait_deadlines());
    serving.spawn(Arc::clone(hub).watch_pull_requests());
    serving.spawn(Arc::clone(hub).observe_github());
    // What it would record is checked again as the next hub starts.
    serving.spawn(Arc::clone(hub).watch_repo_links());
    serving.spawn(Arc::clone(hub).watch_github_reach());
    serving.spawn(Arc::clone(hub).look_for_updates());
}

/// Gives `hub` the update feed, if it has one and this build is a release, since a build of a commit has no version
/// to compare with the feed's.
fn read_update_feed(hub: &Hub, update_feed: Option<UpdateFeed>, build: Build) {
    match update_feed {
        Some(feed) if update_feed::is_release(build.version()) => hub.updates.read_from(feed, build.version()),
        Some(_) => info!(build = build.version(), "not reading the update feed, since this build isn't a release"),
        None => {}
    }
}

/// Waits until the hub is asked to go, and says whether a signal asked. Whoever sends one, such as systemd, can't say
/// whether another hub is coming, so a signal stops the agents, even once the hub was asked to restart.
/// *Unconfirmed:* that a signal stops them.
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

fn open_store(paths: &Paths) -> Result<Arc<Store>, HubError> {
    // Made here so it has its mode before SQLite opens it, which gives its -wal and -shm files the same one.
    private_files::file_options()
        .create(true)
        .append(true)
        .open(&paths.database)
        .map_err(|e| HubError::io(format!("creating {}", paths.database.display()), e))?;
    Ok(Arc::new(Store::open(&paths.database, &paths.backups_dir).map_err(HubError::OpenDatabase)?))
}
