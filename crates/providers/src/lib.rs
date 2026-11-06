//! A provider gives a hub somewhere to run workers. Running on the same machine is one provider,
//! and nothing in the hub treats it as special.

pub mod local;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// A stopped instance keeps its files.
    pub filesystem_survives_stop: bool,
    pub snapshots: bool,
}

/// The bytes between a hub and a worker. Workers need nothing more than this, so an adapter can
/// use stdio, SSH, `incus exec` or anything else.
pub struct WorkerStream {
    pub reader: Box<dyn AsyncRead + Send + Unpin>,
    pub writer: Box<dyn AsyncWrite + Send + Unpin>,
}

/// Instance IDs are chosen by the hub, one per workspace. `snapshot` and `restore` come with the
/// first provider that needs them, declared through [`Capabilities`].
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    /// Makes the instance and puts the worker binary in it.
    async fn create(&self, instance_id: &str) -> anyhow::Result<()>;
    async fn start(&self, instance_id: &str) -> anyhow::Result<()>;
    /// Safe to call on an instance that isn't running. Also ends anything the worker left behind,
    /// so nothing from this run can still be working when the next one starts.
    async fn stop(&self, instance_id: &str) -> anyhow::Result<()>;
    /// Like `stop`, without giving the worker time to finish. For one that isn't responding.
    async fn kill(&self, instance_id: &str) -> anyhow::Result<()>;
    /// Can be called once per `start`.
    async fn connect(&self, instance_id: &str) -> anyhow::Result<WorkerStream>;
    /// Safe to call on an instance that doesn't exist.
    async fn destroy(&self, instance_id: &str) -> anyhow::Result<()>;
}
