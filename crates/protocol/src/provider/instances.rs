//! What a hub asks a provider's adapter to do with its instances, and how the hub reaches each one's worker.

use serde::{Deserialize, Serialize};

use super::{ProviderRequest, ProviderResponse, Reply, Request};
use crate::model::{InstanceId, WorkerTarget};

/// Makes the instance, as a copy of the base image `from` or a plain one, or does nothing if it's there.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Create {
    pub instance_id: InstanceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<InstanceId>,
    /// Ignored by a provider without [`Feature::Sizes`](super::Feature::Sizes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Size>,
    /// What the instance is called in the provider's own tools, such as `roundhouse-holly`.
    pub display_name: String,
}

impl Request for Create {
    type Reply = ();

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::Create(self)
    }
}

/// Starts it, or does nothing to one that's running, whatever runs there. First puts in place the one of `workers`
/// built for the instance's target, if the instance holds a different one, so a worker always runs its hub's build.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Start {
    pub instance_id: InstanceId,
    pub workers: Vec<WorkerBinary>,
}

impl Request for Start {
    type Reply = ();

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::Start(self)
    }
}

/// Ends whatever runs on it, which can be a worker an earlier hub left, and leaves it awake. Putting it to sleep is a
/// request of its own, which is yet to come. Does nothing to one that's stopped or gone. The hub closes its own
/// worker's stream and gives it time to stop first.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stop {
    pub instance_id: InstanceId,
}

impl Request for Stop {
    type Reply = ();

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::Stop(self)
    }
}

/// Like [`Stop`], without giving a worker that isn't responding time to finish.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Kill {
    pub instance_id: InstanceId,
}

impl Request for Kill {
    type Reply = ();

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::Kill(self)
    }
}

/// Removes it, or does nothing if it's gone. A base image can be destroyed while instances made from it run.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Destroy {
    pub instance_id: InstanceId,
}

impl Request for Destroy {
    type Reply = ();

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::Destroy(self)
    }
}

/// Every instance the adapter holds for this hub.
#[derive(Clone, Copy, Debug)]
pub struct List;

impl Request for List {
    type Reply = Instances;

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::List
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Instances {
    pub instances: Vec<Instance>,
}

impl Reply for Instances {
    fn read(response: ProviderResponse) -> Result<Self, ProviderResponse> {
        let ProviderResponse::Instances(instances) = response else { return Err(response) };
        Ok(instances)
    }
}

/// The command whose stdin and stdout are the worker's stream. The hub runs it, so the adapter is never in the
/// stream's way. It can be asked again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Connect {
    pub instance_id: InstanceId,
}

impl Request for Connect {
    type Reply = WorkerCommand;

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::Connect(self)
    }
}

/// Turns a stopped instance into a base image, which other instances are made from and which can't start again, or
/// does nothing if it's one already. Refused with `Unsupported` by a provider without
/// [`Feature::BaseImages`](super::Feature::BaseImages).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MakeBaseImage {
    pub instance_id: InstanceId,
}

impl Request for MakeBaseImage {
    type Reply = ();

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::MakeBaseImage(self)
    }
}

/// What an instance is given. A provider takes the nearest it offers.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cpus: u32,
    pub memory_mib: u32,
}

/// A worker binary of the hub's build, on the hub's machine, where the adapter runs too.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WorkerBinary {
    pub target: WorkerTarget,
    pub path: String,
    /// Of the file, in hex, so the adapter can tell whether an instance already holds it.
    pub sha256: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    pub instance_id: InstanceId,
    pub state: InstanceState,
    #[serde(default)]
    pub base_image: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstanceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    #[serde(other)]
    Unknown,
}

/// A program and its arguments, so a command can't be empty, and what to add to its environment.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WorkerCommand {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

impl Reply for WorkerCommand {
    fn read(response: ProviderResponse) -> Result<Self, ProviderResponse> {
        let ProviderResponse::Command(command) = response else { return Err(response) };
        Ok(command)
    }
}
