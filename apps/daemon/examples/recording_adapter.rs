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
use providers::{Capabilities, Instance, Provider, ProviderError, Size, WorkerBinary, WorkerCommand};

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
    /// Where it writes the name each instance is made with.
    #[arg(long)]
    made: PathBuf,
}

struct Recording {
    local: LocalProvider,
    made: PathBuf,
}

#[async_trait]
impl Provider for Recording {
    fn name(&self) -> &'static str {
        "recording"
    }

    fn capabilities(&self) -> Capabilities {
        self.local.capabilities()
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
        self.local.destroy(instance_id).await
    }

    async fn list(&self) -> Result<Vec<Instance>, ProviderError> {
        self.local.list().await
    }

    async fn connect(&self, instance_id: &InstanceId) -> Result<WorkerCommand, ProviderError> {
        self.local.connect(instance_id).await
    }

    async fn make_base_image(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.local.make_base_image(instance_id).await
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let local = LocalProvider::new(LocalConfig {
        worker_exe: args.worker,
        instances_dir: args.instances,
        workspaces_root: args.workspaces_root,
        // So it doesn't wait on the login shell of whoever runs the tests.
        shell: Some("/bin/sh".to_string()),
        path: std::env::var("PATH").ok(),
    });
    let provider: Arc<dyn Provider> = Arc::new(Recording { local, made: args.made });
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let build = protocol::this_build!().version();
    providers::serve::serve(Box::new(move |_| Ok(provider)), build, stdin, stdout).await.unwrap();
}
