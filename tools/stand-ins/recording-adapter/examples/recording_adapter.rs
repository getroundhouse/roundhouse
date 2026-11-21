//! Stands in for the adapter of a provider other than the local one, which a test can't otherwise get. It serves the
//! local provider over the provider protocol, as an adapter does, and writes the ID of each instance it makes to
//! `--made`, and of each it's asked how to reach a worker on to `--connected`, one to a line, since the local provider
//! has no tools of its own to show either in. An instance that's there already isn't made again, as with any provider,
//! so asking for it again writes nothing. The workers it's given at each start go beside `--made`, as `.started`.

#![expect(clippy::unwrap_used, reason = "a test program, so failing loudly is what it should do")]

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use clap::Parser;
use protocol::model::InstanceId;
use providers::local::{LocalConfig, LocalProvider};
use providers::{
    Capabilities, ClonePersistence, Instance, Provider, ProviderError, Size, WorkerBinary, WorkerCommand,
    WorkerEnvironment, WorkerFilesystem,
};

#[derive(Parser)]
struct Args {
    /// The binary that runs `worker --stdio`.
    #[arg(long)]
    worker: PathBuf,
    /// Where the local provider keeps its instances, apart from the hub's own local provider's.
    #[arg(long)]
    instances: PathBuf,
    /// The hub's, where workers make worktrees.
    #[arg(long)]
    workspaces_root: PathBuf,
    /// The hub's, where workers keep their clones.
    #[arg(long)]
    clones: PathBuf,
    /// Where it writes the ID of each instance it makes.
    #[arg(long)]
    made: PathBuf,
    /// Where it writes the ID of each instance it's asked how to reach a worker on.
    #[arg(long)]
    connected: PathBuf,
    /// Declare `WorkerFilesystem::Separate` instead of sharing the hub's filesystem.
    #[arg(long)]
    separate: bool,
    /// Declare `ClonePersistence::Ephemeral` and wipe `--clones` on `destroy` to simulate an ephemeral VM.
    #[arg(long)]
    ephemeral: bool,
}

struct Recording {
    local: LocalProvider,
    made: PathBuf,
    connected: PathBuf,
    clones_dir: PathBuf,
    environment: WorkerEnvironment,
}

#[async_trait]
impl Provider for Recording {
    fn name(&self) -> &'static str {
        "recording"
    }

    fn capabilities(&self) -> Capabilities {
        self.local.capabilities()
    }

    async fn environment(&self) -> Result<WorkerEnvironment, ProviderError> {
        Ok(self.environment)
    }

    fn starts_or_stops_within(&self) -> std::time::Duration {
        self.local.starts_or_stops_within()
    }

    async fn create(
        &self,
        instance_id: &InstanceId,
        from: Option<&InstanceId>,
        size: Option<Size>,
    ) -> Result<(), ProviderError> {
        let there = self.local.list().await?.iter().any(|instance| &instance.instance_id == instance_id);
        self.local.create(instance_id, from, size).await?;
        if !there {
            note(&self.made, instance_id);
        }
        Ok(())
    }

    async fn start(&self, instance_id: &InstanceId, workers: &[WorkerBinary]) -> Result<(), ProviderError> {
        let mut started = std::fs::OpenOptions::new().create(true).append(true).open(self.started()).unwrap();
        started.write_all(&protocol::frame::encode(&workers).unwrap()).unwrap();
        self.local.start(instance_id, workers).await
    }

    async fn end_worker(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.local.end_worker(instance_id).await
    }

    async fn kill_worker(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.local.kill_worker(instance_id).await
    }

    async fn destroy(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.local.destroy(instance_id).await?;
        if self.environment.clones == ClonePersistence::Ephemeral {
            let _ = std::fs::remove_dir_all(&self.clones_dir);
        }
        Ok(())
    }

    async fn list(&self) -> Result<Vec<Instance>, ProviderError> {
        self.local.list().await
    }

    async fn connect(&self, instance_id: &InstanceId) -> Result<WorkerCommand, ProviderError> {
        note(&self.connected, instance_id);
        self.local.connect(instance_id).await
    }

    async fn open_preflight(
        &self,
        launch: &protocol::provider::OpenPreflight,
    ) -> Result<protocol::provider::Command, ProviderError> {
        std::fs::write(self.made.with_extension("preflight"), protocol::frame::encode(launch).unwrap()).unwrap();
        self.local.open_preflight(launch).await
    }

    async fn make_base_image(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.local.make_base_image(instance_id).await
    }
}

impl Recording {
    /// Where it writes the workers it's given at each start, as a line of JSON.
    fn started(&self) -> PathBuf {
        self.made.with_extension("started")
    }
}

/// Adds `instance_id` to the end of the file at `path`, on a line of its own.
fn note(path: &std::path::Path, instance_id: &InstanceId) {
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
    writeln!(file, "{instance_id}").unwrap();
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let clones_dir = args.clones.clone();
    let local = LocalProvider::new(LocalConfig {
        worker_exe: args.worker,
        instances_dir: args.instances,
        workspaces_root: args.workspaces_root,
        clones_dir: args.clones,
    });
    let environment = WorkerEnvironment {
        filesystem: if args.separate { WorkerFilesystem::Separate } else { WorkerFilesystem::Hub },
        clones: if args.ephemeral { ClonePersistence::Ephemeral } else { ClonePersistence::Persistent },
    };
    let provider: Arc<dyn Provider> =
        Arc::new(Recording { local, made: args.made, connected: args.connected, clones_dir, environment });
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let build = protocol::this_build!().version();
    providers::serve::serve(Box::new(move |_| Ok(provider)), build, stdin, stdout).await.unwrap();
}
