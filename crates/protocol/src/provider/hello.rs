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
