//! The provider: one sandbox for each instance, made from a stock image the setup script sets up, which runs the
//! worker the hub gives it.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use microsandbox::sandbox::{SandboxHandle, SandboxStatus};
use microsandbox::{MicrosandboxError, Sandbox};
use protocol::model::{HubId, InstanceId};
use providers::{
    Capabilities, ClonePersistence, Feature, Instance, InstanceState, Provider, ProviderError, Size, WorkerBinary,
    WorkerCommand, WorkerEnvironment, WorkerFilesystem,
};
use tracing::info;

use crate::guest;
use crate::naming::{HUB_LABEL, Names};

/// What a sandbox gets when the hub doesn't say.
const DEFAULT_SIZE: Size = Size { cpus: 4, memory_mib: 4096 };

/// Booting a sandbox takes seconds, and putting a new worker in it, which is tens of megabytes, a few more.
const STARTS_OR_STOPS_WITHIN: Duration = Duration::from_mins(2);

/// How long the worker and its agents get to stop by themselves before they're killed, as on the local provider.
const END_GRACE: Duration = Duration::from_secs(3);

/// The settings the adapter is run with.
pub(crate) struct Settings {
    /// The OCI image each sandbox is made from.
    pub(crate) image: String,
    /// The script that sets it up, as root, once it has booted.
    pub(crate) setup: String,
    /// The `msb` that runs the VMs, which the command that reaches a worker is given too.
    pub(crate) msb: Option<PathBuf>,
}

pub(crate) struct Microsandbox {
    names: Names,
    hub_id: HubId,
    settings: Settings,
}

impl Microsandbox {
    pub(crate) fn new(hub_id: &HubId, settings: Settings) -> Self {
        Self { names: Names::of(hub_id), hub_id: hub_id.clone(), settings }
    }

    /// The instance's sandbox, if there is one.
    async fn handle(&self, instance_id: &InstanceId) -> Result<Option<SandboxHandle>, ProviderError> {
        match Sandbox::get(&self.names.sandbox(instance_id)).await {
            Ok(handle) => Ok(Some(handle)),
            Err(MicrosandboxError::SandboxNotFound(_)) => Ok(None),
            Err(e) => Err(failed("finding the sandbox", &e)),
        }
    }

    /// Makes the instance's sandbox from the image, and boots it.
    async fn make(&self, instance_id: &InstanceId, Size { cpus, memory_mib }: Size) -> Result<Sandbox, ProviderError> {
        let name = self.names.sandbox(instance_id);
        info!(sandbox = %name, image = %self.settings.image, cpus, memory_mib, "making a sandbox");
        Sandbox::builder(&name)
            .image(self.settings.image.as_str())
            .cpus(u8::try_from(cpus).unwrap_or(u8::MAX))
            .memory(memory_mib)
            .label(HUB_LABEL, self.hub_id.as_str())
            .create_detached()
            .await
            .map_err(|e| failed("making the sandbox", &e))
    }

    /// Sets the sandbox up, unless it was set up before.
    async fn set_up(&self, sandbox: &Sandbox) -> Result<(), ProviderError> {
        if run(sandbox, "checking it's set up", &format!("test -e {}", guest::SET_UP)).await.is_ok() {
            return Ok(());
        }
        run(sandbox, "setting the sandbox up", &self.settings.setup).await?;
        run(sandbox, "noting it's set up", &format!("touch {}", guest::SET_UP)).await?;
        Ok(())
    }

    /// Ends what the worker's user runs, if the sandbox is running.
    async fn end(&self, instance_id: &InstanceId, grace: Duration) -> Result<(), ProviderError> {
        let Some(handle) = self.handle(instance_id).await? else { return Ok(()) };
        if handle.status_snapshot() != SandboxStatus::Running {
            return Ok(());
        }
        let sandbox = handle.connect().await.map_err(|e| failed("connecting to the sandbox", &e))?;
        let ended = run(&sandbox, "ending the worker", &guest::end_user_processes(grace)).await;
        sandbox.detach().await;
        ended.map(drop)
    }
}

#[async_trait]
impl Provider for Microsandbox {
    fn name(&self) -> &'static str {
        "microsandbox"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { targets: vec![guest_target().into()], features: vec![Feature::Sizes] }
    }

    async fn environment(&self) -> Result<WorkerEnvironment, ProviderError> {
        Ok(WorkerEnvironment { filesystem: WorkerFilesystem::Separate, clones: ClonePersistence::Ephemeral })
    }

    fn starts_or_stops_within(&self) -> Duration {
        STARTS_OR_STOPS_WITHIN
    }

    async fn create(
        &self,
        instance_id: &InstanceId,
        from: Option<&InstanceId>,
        size: Option<Size>,
    ) -> Result<(), ProviderError> {
        if from.is_some() {
            return Err(ProviderError::Unsupported("make an instance from a base image"));
        }
        let sandbox = if let Some(handle) = self.handle(instance_id).await? {
            handle.connect_or_start_detached().await.map_err(|e| failed("starting the sandbox", &e))?
        } else {
            self.make(instance_id, size.unwrap_or(DEFAULT_SIZE)).await?
        };
        let set_up = self.set_up(&sandbox).await;
        sandbox.detach().await;
        set_up
    }

    /// Puts the worker for its target in place if it holds another, by its checksum, under another name first and then
    /// renamed, so a worker running from the one there goes on with the file it started from.
    async fn start(&self, instance_id: &InstanceId, workers: &[WorkerBinary]) -> Result<(), ProviderError> {
        let handle = self.handle(instance_id).await?.ok_or_else(|| ProviderError::NotFound(instance_id.clone()))?;
        let target = guest_target();
        let worker = workers.iter().find(|worker| worker.target.as_str() == target).ok_or_else(|| {
            let why = format!("the hub gave no worker for {target}, which its providers.json names under workers");
            ProviderError::io("starting the sandbox", io::Error::other(why))
        })?;
        let sandbox = handle.connect_or_start_detached().await.map_err(|e| failed("starting the sandbox", &e))?;
        let placed = place_worker(&sandbox, worker).await;
        sandbox.detach().await;
        placed
    }

    async fn end_worker(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end(instance_id, END_GRACE).await
    }

    async fn kill_worker(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end(instance_id, Duration::ZERO).await
    }

    async fn destroy(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        let Some(handle) = self.handle(instance_id).await? else { return Ok(()) };
        if handle.status_snapshot() == SandboxStatus::Running {
            handle.stop().await.map_err(|e| failed("stopping the sandbox", &e))?;
        }
        match Sandbox::remove(&self.names.sandbox(instance_id)).await {
            Ok(()) | Err(MicrosandboxError::SandboxNotFound(_)) => Ok(()),
            Err(e) => Err(failed("removing the sandbox", &e)),
        }
    }

    async fn list(&self) -> Result<Vec<Instance>, ProviderError> {
        let mut instances = Vec::new();
        let mut cursor = None;
        loop {
            let page = Sandbox::list_with(|list| {
                let list = list.label(HUB_LABEL, self.hub_id.as_str());
                match cursor.take() {
                    Some(cursor) => list.cursor(cursor),
                    None => list,
                }
            })
            .await
            .map_err(|e| failed("listing the sandboxes", &e))?;
            for handle in page.sandboxes {
                let Some(instance_id) = self.names.instance(handle.name()) else { continue };
                let state = state_of(handle.status_snapshot());
                let worker_running = state == InstanceState::Running && worker_running(&handle).await;
                instances.push(Instance { instance_id, state, base_image: false, worker_running });
            }
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => return Ok(instances),
            }
        }
    }

    /// This program, run as `connect`, which streams its stdin and stdout to and from the worker in the sandbox.
    async fn connect(&self, instance_id: &InstanceId) -> Result<WorkerCommand, ProviderError> {
        if self.handle(instance_id).await?.is_none() {
            return Err(ProviderError::NotFound(instance_id.clone()));
        }
        let program = std::env::current_exe().map_err(|e| ProviderError::io("finding this adapter's program", e))?;
        let mut args = vec!["connect".to_owned(), self.names.sandbox(instance_id)];
        if let Some(msb) = &self.settings.msb {
            args.extend(["--msb".to_owned(), msb.to_string_lossy().into_owned()]);
        }
        Ok(WorkerCommand { program: program.to_string_lossy().into_owned(), args, env: Vec::new() })
    }

    async fn make_base_image(&self, _instance_id: &InstanceId) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported("make base images"))
    }
}

/// The target of the workers its sandboxes run: Linux, on the processor of the machine the adapter runs on.
fn guest_target() -> &'static str {
    if cfg!(target_arch = "aarch64") { "aarch64-unknown-linux-gnu" } else { "x86_64-unknown-linux-gnu" }
}

fn state_of(status: SandboxStatus) -> InstanceState {
    match status {
        SandboxStatus::Running => InstanceState::Running,
        SandboxStatus::Starting => InstanceState::Starting,
        SandboxStatus::Draining => InstanceState::Stopping,
        SandboxStatus::Created | SandboxStatus::Paused | SandboxStatus::Stopped | SandboxStatus::Crashed => {
            InstanceState::Stopped
        }
    }
}

/// Whether a worker serves the running sandbox. One that can't be asked is said to have none, so the hub starts one.
async fn worker_running(handle: &SandboxHandle) -> bool {
    let Ok(sandbox) = handle.connect().await else { return false };
    let running = run(&sandbox, "asking for the worker", guest::WORKER_RUNNING).await.is_ok();
    sandbox.detach().await;
    running
}

async fn place_worker(sandbox: &Sandbox, worker: &WorkerBinary) -> Result<(), ProviderError> {
    let there = run(sandbox, "reading the worker's checksum", guest::WORKER_SHA256).await?;
    if there.trim() == worker.sha256 {
        return Ok(());
    }
    let bytes = tokio::fs::read(&worker.path)
        .await
        .map_err(|e| ProviderError::io(format!("reading the worker at {}", worker.path), e))?;
    let partial = format!("{}.partial", guest::WORKER);
    sandbox.fs().write(&partial, bytes).await.map_err(|e| failed("putting the worker in the sandbox", &e))?;
    run(sandbox, "putting the worker in place", &format!("chmod 755 {partial} && mv {partial} {}", guest::WORKER))
        .await?;
    info!(sha256 = %worker.sha256, "put the hub's worker in a sandbox");
    Ok(())
}

/// Runs `script` as root, and says what it printed, or, as `what` failing, what it wrote to stderr last.
async fn run(sandbox: &Sandbox, what: &str, script: &str) -> Result<String, ProviderError> {
    let output = sandbox.shell(script).await.map_err(|e| failed(what, &e))?;
    if output.status().success {
        return Ok(String::from_utf8_lossy(output.stdout_bytes()).into_owned());
    }
    let stderr = String::from_utf8_lossy(output.stderr_bytes());
    let lines: Vec<&str> = stderr.trim().lines().collect();
    let last = lines[lines.len().saturating_sub(5)..].join("\n");
    Err(ProviderError::io(what, io::Error::other(format!("it exited {}: {last}", output.status().code))))
}

fn failed(what: &str, e: &MicrosandboxError) -> ProviderError {
    ProviderError::io(what, io::Error::other(e.to_string()))
}
