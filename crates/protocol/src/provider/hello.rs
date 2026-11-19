//! How a hub and a provider's adapter find a version they share, and what the adapter says it can do.

use serde::{Deserialize, Serialize};

use super::{ProviderRequest, ProviderResponse, Reply, Request};
use crate::model::{HubId, WorkerTarget};

/// Always first. Names the versions the hub speaks, newest and oldest, and the hub whose instances the adapter works
/// with from then on. The same hub's hello again gets the same answer, and another hub's is refused.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hello {
    pub protocol_version: u32,
    pub oldest_protocol_version: u32,
    pub hub_id: HubId,
    pub build: String,
}

impl Request for Hello {
    type Reply = HelloReply;

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::Hello(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HelloReply {
    /// The version chosen, which is the newest both sides speak.
    pub protocol_version: u32,
    pub adapter: String,
    pub build: String,
    pub capabilities: Capabilities,
    /// What the compute this provider's workers run on looks like to the hub, separate from the operations the
    /// provider itself supports in `capabilities`. Omitted when workers run on separate ephemeral compute, which is
    /// the safe default for an adapter that doesn't say.
    #[serde(default, skip_serializing_if = "WorkerEnvironment::is_default")]
    pub environment: WorkerEnvironment,
}

/// What the compute a provider's workers run on looks like to the hub.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkerEnvironment {
    /// Whether workers can reach checkouts on the hub's own filesystem.
    #[serde(default)]
    pub filesystem: WorkerFilesystem,
    /// Whether a repository's clone on the worker's machine survives archiving a workspace (`destroy` then `create`).
    #[serde(default)]
    pub clones: ClonePersistence,
}

impl WorkerEnvironment {
    /// The local provider's environment: workers run on the hub's machine and share its clones directory.
    pub const HUB_MACHINE: Self = Self { filesystem: WorkerFilesystem::Hub, clones: ClonePersistence::Persistent };

    #[expect(clippy::trivially_copy_pass_by_ref, reason = "serde's skip_serializing_if passes &T")]
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Whether a provider's workers share the hub's own filesystem.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerFilesystem {
    /// Separate from the hub's machine: workers clone repositories by address and see `CONDUCTOR_IS_LOCAL=0`.
    #[default]
    Separate,
    /// Shared with the hub's machine: workers can use a checkout on the hub and see `CONDUCTOR_IS_LOCAL=1`.
    Hub,
    #[serde(other)]
    Unknown,
}

/// Whether a repository's clone on a provider's worker machine survives archiving a workspace (`destroy` then `create`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClonePersistence {
    /// Destroying an instance may lose the clone (ephemeral VMs/sandboxes, or a pool of machines).
    #[default]
    Ephemeral,
    /// A repository's clone survives archiving a workspace and is reused when restoring it.
    Persistent,
    #[serde(other)]
    Unknown,
}

impl Reply for HelloReply {
    fn read(response: ProviderResponse) -> Result<Self, ProviderResponse> {
        let ProviderResponse::Hello(hello) = response else { return Err(response) };
        Ok(hello)
    }
}

/// What a provider can do, declared at hello.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Of the workers its instances run.
    #[serde(default)]
    pub targets: Vec<WorkerTarget>,
    /// Whatever an older adapter leaves out, it can't do.
    #[serde(default)]
    pub features: Vec<Feature>,
}

impl Capabilities {
    pub fn has(&self, feature: Feature) -> bool {
        self.features.contains(&feature)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    /// A stopped instance keeps its files.
    FilesystemSurvivesStop,
    /// It takes the `size` a `Create` asks for.
    Sizes,
    /// It answers `MakeBaseImage`, and makes instances `from` a base image.
    BaseImages,
    Snapshots,
    #[serde(other)]
    Unknown,
}
