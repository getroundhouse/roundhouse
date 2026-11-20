//! Stands in for the adapter of a provider other than the local one, which a test can't otherwise get. It serves the
//! local provider over the provider protocol, as an adapter does, and writes the name each instance is made with to
//! `--made`, one to a line, since the local provider has no tools of its own to show it in. An instance that's there
//! already isn't made again, as with any provider, so asking for it again under another name writes nothing.

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
    /// Where it writes the name each instance is made with.
    #[arg(long)]
    made: PathBuf,
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
        display_name: &str,
    ) -> Result<(), ProviderError> {
        let there = self.local.list().await?.iter().any(|instance| &instance.instance_id == instance_id);
        self.local.create(instance_id, from, size, display_name).await?;
        if !there {
            let mut made = std::fs::OpenOptions::new().create(true).append(true).open(&self.made).unwrap();
            writeln!(made, "{display_name}").unwrap();
        }
        Ok(())
    }

    async fn start(&self, instance_id: &InstanceId, workers: &[WorkerBinary]) -> Result<(), ProviderError> {
        self.local.start(instance_id, workers).await
    }

    async fn stop(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.local.stop(instance_id).await
    }

    async fn kill(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.local.kill(instance_id).await
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

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let clones_dir = args.clones.clone();
    let local = LocalProvider::new(LocalConfig {
        worker_exe: args.worker,
        instances_dir: args.instances,
        workspaces_root: args.workspaces_root,
        clones_dir: args.clones,
        // So it doesn't wait on the login shell of whoever runs the tests.
        path: providers::local::MachinePath::new(Some("/bin/sh".to_string()), std::env::var("PATH").ok()),
    });
    let environment = WorkerEnvironment {
        filesystem: if args.separate { WorkerFilesystem::Separate } else { WorkerFilesystem::Hub },
        clones: if args.ephemeral { ClonePersistence::Ephemeral } else { ClonePersistence::Persistent },
    };
    let provider: Arc<dyn Provider> = Arc::new(Recording { local, made: args.made, clones_dir, environment });
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let build = protocol::this_build!().version();
    providers::serve::serve(Box::new(move |_| Ok(provider)), build, stdin, stdout).await.unwrap();
}
