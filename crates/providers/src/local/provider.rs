use crate::{
    Capabilities, Feature, Instance, InstanceState, Provider, ProviderError, Size, WorkerBinary, WorkerCommand,
    WorkerLiveness, WorkerReach,
};
use async_trait::async_trait;
use process::{Environment, ProcessRecord, Signal, signal_group};
use protocol::dialing::WorkerTicket;
use protocol::model::{InstanceId, ProviderName};
use protocol::paths::{AGENTS_DIR, WORKER_RECORD_FILE};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{info, warn};

/// How long what a killed worker left running, such as an agent, or a worker still running when the hub asks for it
/// to be ended, gets to stop by itself, with all it started. An agent stops its own tools if given this.
const ORPHAN_GRACE: Duration = Duration::from_secs(3);
/// How long a process sent SIGKILL gets to be gone.
const KILLED_GRACE: Duration = Duration::from_secs(2);
/// How long starting an instance, or ending its worker, takes at most. Either stops each process left there within its
/// [`ORPHAN_GRACE`] and then [`KILLED_GRACE`]. So this is that, with three processes left there.
const STARTS_OR_STOPS_WITHIN: Duration = Duration::from_secs(3 * (ORPHAN_GRACE.as_secs() + KILLED_GRACE.as_secs()));

pub struct LocalConfig {
    /// The binary that runs a worker, as `worker --ticket`, or `worker --stdio` where they don't dial.
    pub worker_exe: PathBuf,
    /// One directory per instance, holding its worker's records.
    pub instances_dir: PathBuf,
    pub workspaces_root: PathBuf,
    /// Where every worker keeps its clones, which they share.
    pub clones_dir: PathBuf,
    /// Where each worker that dials writes its log, a file per instance, as `protocol::paths::worker_log` names it.
    pub worker_logs_dir: PathBuf,
    /// What a worker it starts is given over what it inherits from the hub, which is the machine's environment.
    pub environment: Environment,
    /// Whether its workers dial the hub, at its endpoint on loopback, and outlive it, as every hub's do, or the hub
    /// runs `worker --stdio` and they stop with it. Only the hub's own tests turn it off, to stand a script in for a
    /// worker on its stdin and stdout, which couldn't dial.
    pub dials: bool,
    /// Where the hub writes its loopback endpoint's address, which a worker that dials reads once a dial has failed,
    /// to find a hub that came back on another port. `None` for a worker that dials another endpoint, as an adapter's
    /// in a test does.
    pub hub_address_file: Option<PathBuf>,
}

pub struct LocalProvider {
    pub(super) config: LocalConfig,
}

impl LocalProvider {
    pub fn new(config: LocalConfig) -> Self {
        Self { config }
    }

    pub(super) fn instance_dir(&self, instance_id: &InstanceId) -> PathBuf {
        self.config.instances_dir.join(instance_id.as_str())
    }

    /// Ends the instance's worker, if one is still running, and then whatever a worker left behind.
    pub(super) async fn end(&self, instance_id: &InstanceId, grace: Duration) {
        end_in(&self.instance_dir(instance_id), grace).await;
    }
}

/// Ends the worker whose state directory is `dir`, if one is still running, giving it `grace` to stop by itself, and
/// then whatever a worker left behind there, such as its agents. What `roundhoused end-worker` does on a machine of any
/// provider's, so finding and ending a worker's processes has one home.
pub async fn end_worker_in(dir: &Path, now: bool) {
    end_in(dir, if now { Duration::ZERO } else { ORPHAN_GRACE }).await;
}

async fn end_in(dir: &Path, grace: Duration) {
    if let Some(worker) = running_worker(dir).await {
        info!(name: "worker.left_running", pid = worker.pid, dir = %dir.display(), "Ending a worker still running in its instance");
        stop_groups(&[worker], grace).await;
    }
    stop_orphans(dir, ORPHAN_GRACE).await;
}

/// The worker serving the instance in `dir`, which records itself there once it holds the instance's lock.
async fn running_worker(dir: &Path) -> Option<ProcessRecord> {
    let worker = ProcessRecord::read(&dir.join(WORKER_RECORD_FILE)).filter(|worker| worker.pid > 1)?;
    still_running(&worker).await.then_some(worker)
}

/// Asks the process groups `leaders` lead to stop, all at once, so each gets the whole of `grace`, and makes whatever is
/// left of them stop after it, or at once if that's zero. Returns once they've gone. A group can outlast its leader, as
/// a server started by a script that exits on SIGTERM does, so it's gone once nothing is left of it, as
/// [`ProcessRecord::group_running`] says, and is set aside then, since its ID can be another's after.
async fn stop_groups(leaders: &[ProcessRecord], grace: Duration) {
    let mut left: Vec<&ProcessRecord> = leaders.iter().collect();
    if !grace.is_zero() {
        for leader in &left {
            signal_group(leader.pid, Signal::Terminate);
        }
        if gone_within(&mut left, grace).await {
            return;
        }
    }
    left.retain(|leader| leader.group_running());
    for leader in &left {
        warn!(
            name: "process.group_not_stopped",
            pid = leader.pid,
            "A group is still running after SIGTERM, so it's sent SIGKILL"
        );
        signal_group(leader.pid, Signal::Kill);
    }
    if !gone_within(&mut left, KILLED_GRACE).await {
        warn!(name: "process.group_not_killed", "A group is still running after SIGKILL");
    }
}

/// Whether every group in `left` has gone within `grace`, setting each aside as it goes.
#[expect(
    clippy::disallowed_methods,
    reason = "the pace groups that have been asked to stop are looked at again until they've gone"
)]
async fn gone_within(left: &mut Vec<&ProcessRecord>, grace: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + grace;
    loop {
        left.retain(|leader| leader.group_running());
        if left.is_empty() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn create_dir(dir: &Path) -> Result<(), ProviderError> {
    std::fs::create_dir_all(dir).map_err(|e| ProviderError::io(format!("creating {}", dir.display()), e))
}

async fn still_running(agent: &ProcessRecord) -> bool {
    let agent = agent.clone();
    tokio::task::spawn_blocking(move || agent.is_running()).await.unwrap_or(false)
}

/// A worker records each process it runs that would outlive it, such as an agent, and removes the record when that
/// process exits. Any record still here means the worker was killed without stopping it. An orphaned agent runs on
/// until its turn ends, with nobody reading its output, and a new agent would be working in the same worktree on the
/// same conversation. So each recorded process that's still the one recorded, by its start time, goes first, with its
/// group, which gets `grace` to stop by itself. A group whose leader has gone is left, since only the leader's start
/// time shows that the record is still that group's. Every other file here is a run's too, such as the one that tells
/// an agent's CLI where the worker's tools are, which holds the agent's token, so everything here goes.
pub(super) async fn stop_orphans(instance_dir: &Path, grace: Duration) {
    let Ok(records) = std::fs::read_dir(instance_dir.join(AGENTS_DIR)) else {
        return;
    };
    let (mut orphans, mut files) = (Vec::new(), Vec::new());
    for record in records.flatten() {
        let path = record.path();
        if let Some(orphan) = ProcessRecord::read(&path).filter(|orphan| orphan.pid > 1)
            && still_running(&orphan).await
        {
            warn!(
                name: "process.orphan_found",
                pid = orphan.pid,
                "Stopping what a worker that was killed left running"
            );
            orphans.push(orphan);
        }
        files.push(path);
    }
    stop_groups(&orphans, grace).await;
    for file in files {
        let _ = std::fs::remove_file(&file);
    }
}

#[async_trait]
impl Provider for LocalProvider {
    fn name(&self) -> &'static str {
        ProviderName::LOCAL
    }

    /// Its workers run the hub's own executable, so it names no targets to be given workers for.
    fn capabilities(&self) -> Capabilities {
        Capabilities { targets: Vec::new(), features: vec![Feature::FilesystemSurvivesStop] }
    }

    async fn environment(&self) -> Result<crate::WorkerEnvironment, ProviderError> {
        Ok(crate::WorkerEnvironment::HUB_MACHINE)
    }

    fn starts_or_stops_within(&self) -> Duration {
        STARTS_OR_STOPS_WITHIN
    }

    async fn create(
        &self,
        instance_id: &InstanceId,
        from: Option<&InstanceId>,
        _size: Option<Size>,
    ) -> Result<(), ProviderError> {
        if from.is_some() {
            return Err(ProviderError::Unsupported("make an instance from a base image"));
        }
        create_dir(&self.instance_dir(instance_id))
    }

    fn workers_reach(&self) -> WorkerReach {
        if self.config.dials { WorkerReach::Loopback } else { WorkerReach::Command }
    }

    /// Its workers run the hub's own executable, so there's none to put in place. A running instance is left as it
    /// is, and one is running while a worker is. Ending that is the hub's to ask for, with `end_worker`. Otherwise it
    /// starts one that dials the hub with `ticket`, if its workers dial.
    async fn start(
        &self,
        instance_id: &InstanceId,
        _workers: &[WorkerBinary],
        ticket: Option<&WorkerTicket>,
    ) -> Result<(), ProviderError> {
        let dir = self.instance_dir(instance_id);
        create_dir(&dir)?;
        if running_worker(&dir).await.is_some() {
            return Ok(());
        }
        // For when the hub was killed along with its worker, so `end_worker` never ran.
        stop_orphans(&dir, ORPHAN_GRACE).await;
        if !self.config.dials {
            return Ok(());
        }
        let ticket = ticket.ok_or(ProviderError::Unsupported("start a worker without a ticket"))?;
        super::starting::start_dialing(&self.config, instance_id, &dir, ticket).await
    }

    async fn end_worker(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end(instance_id, ORPHAN_GRACE).await;
        Ok(())
    }

    async fn kill_worker(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end(instance_id, Duration::ZERO).await;
        Ok(())
    }

    async fn destroy(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end_worker(instance_id).await?;
        let dir = self.instance_dir(instance_id);
        match std::fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(ProviderError::io(format!("removing {}", dir.display()), e))
            }
            _ => Ok(()),
        }
    }

    async fn list(&self) -> Result<Vec<Instance>, ProviderError> {
        let dir = &self.config.instances_dir;
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(ProviderError::io(format!("reading {}", dir.display()), e)),
        };
        let instance_id = |entry: &std::fs::DirEntry| entry.file_name().to_string_lossy().into_owned().into();
        // A directory is never asleep.
        let instances = entries.flatten().filter(|entry| entry.path().is_dir()).map(|entry| Instance {
            instance_id: instance_id(&entry),
            state: InstanceState::Running,
            base_image: false,
        });
        Ok(instances.collect())
    }

    async fn workers_running(&self) -> Result<Vec<InstanceId>, ProviderError> {
        let mut running = Vec::new();
        for instance in self.list().await? {
            if running_worker(&self.instance_dir(&instance.instance_id)).await.is_some() {
                running.push(instance.instance_id);
            }
        }
        Ok(running)
    }

    /// By the claim the worker holds, as [`claim_liveness`] says. A worker an earlier build left listening holds it
    /// too, since every worker takes it, so an instance that was never claimed is unknown.
    async fn worker_state(&self, instance_id: &InstanceId) -> Result<WorkerLiveness, ProviderError> {
        let dir = self.instance_dir(instance_id);
        Ok(tokio::task::spawn_blocking(move || super::claimed::claim_liveness(&dir))
            .await
            .unwrap_or(WorkerLiveness::Unknown))
    }

    /// `worker --attach`, which reaches a worker an earlier build left listening on a socket in its state directory,
    /// and starts none, since every worker this build starts dials the hub. It goes at the next break of the worker
    /// protocol, as `docs/decisions/hub-speaks-previous-version.md` says. `worker --stdio` where workers don't dial.
    async fn connect(&self, instance_id: &InstanceId) -> Result<WorkerCommand, ProviderError> {
        let dir = self.instance_dir(instance_id);
        if !dir.is_dir() {
            return Err(ProviderError::NotFound(instance_id.clone()));
        }
        let text = |path: &Path| path.to_string_lossy().into_owned();
        let transport = if self.config.dials { "--attach" } else { "--stdio" };
        Ok(WorkerCommand {
            program: text(&self.config.worker_exe),
            args: vec![
                "worker".into(),
                transport.into(),
                "--workspaces-root".into(),
                text(&self.config.workspaces_root),
                "--state-dir".into(),
                text(&dir),
                "--clones-dir".into(),
                text(&self.config.clones_dir),
            ],
            // The hub gives every command it runs the machine's environment.
            env: Vec::new(),
        })
    }

    async fn open_preflight(
        &self,
        launch: &crate::OpenPreflight,
    ) -> Result<protocol::provider::Command, ProviderError> {
        let crate::OpenPreflight { project, agent } = launch;
        let checkout = project.checkout().ok_or(ProviderError::Unsupported("open preflight without a checkout"))?;
        Ok(protocol::provider::Command {
            program: self.config.worker_exe.to_string_lossy().into_owned(),
            args: vec!["preflight".into(), agent.as_str().into(), "--".into(), checkout.into()],
            env: Vec::new(),
        })
    }

    async fn make_base_image(&self, _instance_id: &InstanceId) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported("make base images"))
    }
}
