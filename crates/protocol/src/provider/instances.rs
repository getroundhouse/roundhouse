//! What a hub asks a provider's adapter to do with its instances, and how the hub reaches each one's worker.

use serde::{Deserialize, Serialize};

use super::{ProviderRequest, ProviderResponse, Reply, Request};
use crate::dialing::WorkerTicket;
use crate::model::{InstanceId, WorkerTarget};

/// Makes the instance, as a copy of the base image `from` or a plain one, or does nothing if it's there. Its ID is the
/// only name it's given, so a provider's own tools call it by that, which is its workspace's ID: a workspace's name can
/// change while it's being made, and its instance stays as it is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Create {
    pub instance_id: InstanceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<InstanceId>,
    /// Ignored by a provider without [`Feature::Sizes`](super::Feature::Sizes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Size>,
}

impl Request for Create {
    type Reply = ();

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::Create(self)
    }
}

/// Starts it, or does nothing to one that's running, whatever runs there. First puts in place the one of `workers`
/// built for the instance's target, if the instance holds a different one, so a worker always runs its hub's build.
/// A worker it starts is given `ticket`, which it dials the hub with, as [`crate::dialing`] says, so the adapter is
/// never in the worker's stream. A worker already running there dials with the ticket it was given, so this one goes
/// unused.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Start {
    pub instance_id: InstanceId,
    pub workers: Vec<WorkerBinary>,
    pub ticket: WorkerTicket,
}

impl Request for Start {
    type Reply = ();

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::Start(self)
    }
}

/// Ends whatever runs on it, which can be a worker an earlier hub left, and leaves it awake, so it's never what an
/// adapter's own tools call stopping an instance. Putting it to sleep is a request of its own, which is yet to come.
/// Does nothing to one that's stopped or gone. The hub closes its own worker's stream and gives it time to stop first,
/// since that's the same over any transport, so an adapter only ends what's left.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EndWorker {
    pub instance_id: InstanceId,
}

impl Request for EndWorker {
    type Reply = ();

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::EndWorker(self)
    }
}

/// Like [`EndWorker`], without giving a worker that isn't responding time to finish.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct KillWorker {
    pub instance_id: InstanceId,
}

impl Request for KillWorker {
    type Reply = ();

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::KillWorker(self)
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

/// The instance's own state, such as whether its machine is on, whatever runs there.
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
pub struct Command {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

impl Reply for Command {
    fn read(response: ProviderResponse) -> Result<Self, ProviderResponse> {
        let ProviderResponse::Command(command) = response else { return Err(response) };
        Ok(command)
    }
}
