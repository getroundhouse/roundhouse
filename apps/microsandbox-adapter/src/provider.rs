//! The provider: one sandbox for each instance, made from [`guest::IMAGE`] and set up by the adapter's script, with a
//! UDP port forwarded into it, which runs the worker the hub gives it and starts it with its ticket, so it dials the
//! hub itself.

use std::io;
use std::time::Duration;

use async_trait::async_trait;
use microsandbox::sandbox::{SandboxHandle, SandboxStatus};
use microsandbox::{MicrosandboxError, Sandbox};
use protocol::dialing::WorkerTicket;
use protocol::model::{HubId, InstanceId};
use protocol::provider::Hello;
use providers::{
    Capabilities, ClonePersistence, Feature, Instance, InstanceState, Provider, ProviderError, Size, WorkerBinary,
    WorkerEnvironment, WorkerFilesystem,
};
use tracing::{info, warn};

use crate::guest;
use crate::naming::{HUB_LABEL, Names};
use crate::sandbox::{forwarded_port, free_udp_port, run, run_worker};

/// What a sandbox gets when the hub doesn't say.
const DEFAULT_SIZE: Size = Size { cpus: 4, memory_mib: 4096 };

/// Booting a sandbox takes seconds, and putting a new worker in it, which is tens of megabytes, a few more. It's also
/// how long the hub waits for a worker started there to dial it.
const STARTS_OR_STOPS_WITHIN: Duration = Duration::from_mins(2);

/// How long the setup script gets, which installs packages and the agents' command lines, about twenty seconds on the
/// Studio, before it's ended and making the sandbox fails, rather than the hub waiting on it until the provider
/// protocol's own limit.
const SET_UP_WITHIN: Duration = Duration::from_mins(10);

pub(crate) struct Microsandbox {
    names: Names,
    hub_id: HubId,
    /// Which build the hub is, which the worker it gives has to be.
    hub_build: String,
}

impl Microsandbox {
    pub(crate) fn new(hello: &Hello) -> Self {
        Self { names: Names::of(&hello.hub_id), hub_id: hello.hub_id.clone(), hub_build: hello.build.clone() }
    }

    /// The instance's sandbox, if there is one.
    async fn handle(&self, instance_id: &InstanceId) -> Result<Option<SandboxHandle>, ProviderError> {
        match Sandbox::get(&self.names.sandbox(instance_id)).await {
            Ok(handle) => Ok(Some(handle)),
            Err(MicrosandboxError::SandboxNotFound(_)) => Ok(None),
            Err(e) => Err(failed("finding the sandbox", &e)),
        }
    }

    /// Makes the instance's sandbox from the image, with a free UDP port on the hub's machine forwarded to the worker's
    /// in the sandbox, and boots it.
    async fn make(&self, instance_id: &InstanceId, Size { cpus, memory_mib }: Size) -> Result<Sandbox, ProviderError> {
        let name = self.names.sandbox(instance_id);
        let forwarded = free_udp_port()?;
        let vcpus = u8::try_from(cpus).unwrap_or_else(|_| {
            warn!(name: "sandbox.cpus_cut", cpus, "Microsandbox gives a sandbox at most 255 CPUs, so it gets that many");
            u8::MAX
        });
        info!(name: "sandbox.making", sandbox = %name, image = guest::IMAGE, vcpus, memory_mib, forwarded, "Making a sandbox");
        Sandbox::builder(&name)
            .image(guest::IMAGE)
            .cpus(vcpus)
            .memory(memory_mib)
            .port_udp(forwarded, guest::WORKER_PORT)
            .label(HUB_LABEL, self.hub_id.as_str())
            .create_detached()
            .await
            .map_err(|e| failed("making the sandbox", &e))
    }

    /// Sets the sandbox up, unless it was set up before.
    async fn set_up(sandbox: &Sandbox) -> Result<(), ProviderError> {
        if run(sandbox, "checking it's set up", &format!("test -e {}", guest::SET_UP), None).await.is_ok() {
            return Ok(());
        }
        run(sandbox, "setting the sandbox up", guest::SETUP, Some(SET_UP_WITHIN)).await?;
        run(sandbox, "noting it's set up", &format!("touch {}", guest::SET_UP), None).await?;
        Ok(())
    }

    /// Ends the worker and what it left, if the sandbox is running, as the worker finds them.
    async fn end(&self, instance_id: &InstanceId, now: bool) -> Result<(), ProviderError> {
        let Some(handle) = self.handle(instance_id).await? else { return Ok(()) };
        if handle.status_snapshot() != SandboxStatus::Running {
            return Ok(());
        }
        let sandbox = handle.connect().await.map_err(|e| failed("connecting to the sandbox", &e))?;
        let ended = run_worker(&sandbox, "ending the worker", guest::end_worker_args(now), false).await;
        sandbox.detach().await;
        ended.map(drop)
    }

    /// Puts the worker for its target in place if it holds another, checks it's the hub's build, and starts it with
    /// `ticket`, advertising the port forwarded to it. A worker running there already holds the instance, so the one
    /// this starts leaves it to that one and exits.
    async fn start_worker(
        &self,
        sandbox: &Sandbox,
        worker: &WorkerBinary,
        ticket: &WorkerTicket,
    ) -> Result<(), ProviderError> {
        place_worker(sandbox, worker).await?;
        let said = run_worker(sandbox, "asking the worker its build", vec!["--version".to_owned()], false).await?;
        if said.trim() != protocol::paths::daemon_version_line(&self.hub_build) {
            let why = format!(
                "the worker providers.json names for {} says {:?}, and the hub is {}, so the hub would refuse it. \
                 Name the hub's own build there",
                worker.target,
                said.trim(),
                self.hub_build
            );
            return Err(ProviderError::io("starting the worker", io::Error::other(why)));
        }
        let forwarded = forwarded_port(sandbox)?;
        run_worker(sandbox, "starting the worker", guest::worker_args(ticket, forwarded), true).await?;
        info!(name: "worker.started", forwarded, "Started the worker, which dials the hub");
        Ok(())
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
        let set_up = Self::set_up(&sandbox).await;
        sandbox.detach().await;
        set_up
    }

    async fn start(
        &self,
        instance_id: &InstanceId,
        workers: &[WorkerBinary],
        ticket: Option<&WorkerTicket>,
    ) -> Result<(), ProviderError> {
        let ticket = ticket.ok_or(ProviderError::Unsupported("start a worker without a ticket"))?;
        let handle = self.handle(instance_id).await?.ok_or_else(|| ProviderError::NotFound(instance_id.clone()))?;
        let target = guest_target();
        let worker = workers.iter().find(|worker| worker.target.as_str() == target).ok_or_else(|| {
            let why = format!("the hub gave no worker for {target}, which its providers.json names under workers");
            ProviderError::io("starting the sandbox", io::Error::other(why))
        })?;
        let sandbox = handle.connect_or_start_detached().await.map_err(|e| failed("starting the sandbox", &e))?;
        let started = self.start_worker(&sandbox, worker, ticket).await;
        sandbox.detach().await;
        started
    }

    async fn end_worker(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end(instance_id, false).await
    }

    async fn kill_worker(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end(instance_id, true).await
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

    /// From what microsandbox keeps of each sandbox, without asking any, so a starting hub has its answer at once.
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
                instances.push(Instance { instance_id, state, base_image: false });
            }
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => return Ok(instances),
            }
        }
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

/// Under another name first and then renamed, so a worker running from the one there goes on with the file it started
/// from. Kept while its SHA-256, which the hub worked out, matches the one there.
async fn place_worker(sandbox: &Sandbox, worker: &WorkerBinary) -> Result<(), ProviderError> {
    let there = run(sandbox, "reading the worker's checksum", guest::WORKER_SHA256, None).await?;
    if there.trim() == worker.sha256 {
        return Ok(());
    }
    let bytes = tokio::fs::read(&worker.path)
        .await
        .map_err(|e| ProviderError::io(format!("reading the worker at {}", worker.path), e))?;
    let partial = format!("{}.partial", guest::WORKER);
    sandbox.fs().write(&partial, bytes).await.map_err(|e| failed("putting the worker in the sandbox", &e))?;
    let script = format!("chmod 755 {partial} && mv {partial} {}", guest::WORKER);
    run(sandbox, "putting the worker in place", &script, None).await?;
    info!(name: "worker.placed", sha256 = %worker.sha256, "Put the hub's worker in a sandbox");
    Ok(())
}

pub(crate) fn failed(what: &str, e: &MicrosandboxError) -> ProviderError {
    ProviderError::io(what, io::Error::other(e.to_string()))
}
