//! What a hub says to a provider's adapter, over the adapter's stdin and stdout. "Providers" in
//! `docs/architecture.md` has the design: the adapter makes and ends the provider's instances, and the hub
//! reaches each instance's worker itself, by running the command `Connect` answers with.
//!
//! Every request is safe to repeat. `Hello` and `List` get the same answer again, and every other request names
//! its instance and has the same effect however often it's sent. So the hub sends again whatever an adapter that
//! restarted didn't answer.

use serde::{Deserialize, Serialize};

use crate::client::ApiError;
use crate::model::{HubId, InstanceId, WorkerTarget};

/// The provider protocol's own version, apart from [`crate::PROTOCOL_VERSION`], so a break in one doesn't break
/// the other.
pub const PROVIDER_PROTOCOL_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProviderCall {
    pub id: u64,
    pub request: ProviderRequest,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ProviderRequest {
    /// Always first. Names the versions the hub speaks, newest and oldest, and the hub whose instances the
    /// adapter works with from then on. The same hub's hello again gets the same answer, and another hub's is
    /// refused.
    Hello { protocol_version: u32, oldest_protocol_version: u32, hub_id: HubId, build: String },
    /// Makes the instance, as a copy of the base image `from` or a plain one, or does nothing if it's there.
    Create {
        instance_id: InstanceId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<InstanceId>,
        /// Ignored by a provider without [`Feature::Sizes`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<Size>,
        /// What the instance is called in the provider's own tools, such as `roundhouse-holly`.
        display_name: String,
    },
    /// Starts it, or does nothing to one that's running, whatever runs there. First puts in place the one of
    /// `workers` built for the instance's target, if the instance holds a different one, so a worker always runs
    /// its hub's build.
    Start { instance_id: InstanceId, workers: Vec<WorkerBinary> },
    /// Ends whatever runs on it, which can be a worker an earlier hub left, and leaves it awake. Putting it to sleep
    /// is a request of its own, which is yet to come. Does nothing to one that's stopped or gone. The hub closes its
    /// own worker's stream and gives it time to stop first.
    Stop { instance_id: InstanceId },
    /// Like `Stop`, without giving a worker that isn't responding time to finish.
    Kill { instance_id: InstanceId },
    /// Removes it, or does nothing if it's gone. A base image can be destroyed while instances made from it run.
    Destroy { instance_id: InstanceId },
    /// Every instance the adapter holds for this hub.
    List,
    /// The command whose stdin and stdout are the worker's stream. The hub runs it, so the adapter is never in
    /// the stream's way. It can be asked again.
    Connect { instance_id: InstanceId },
    /// Turns a stopped instance into a base image, which other instances are made from and which can't start
    /// again, or does nothing if it's one already. Refused with `Unsupported` by a provider without
    /// [`Feature::BaseImages`].
    MakeBaseImage { instance_id: InstanceId },
    /// A request from a newer hub, which the adapter refuses with `Unsupported`.
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ProviderResponse {
    /// The version chosen, which is the newest both sides speak.
    Hello {
        protocol_version: u32,
        adapter: String,
        build: String,
        capabilities: Capabilities,
    },
    Ok,
    Instances {
        instances: Vec<Instance>,
    },
    Command(WorkerCommand),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum ProviderFrame {
    Reply {
        id: u64,
        #[serde(flatten)]
        outcome: ProviderOutcome,
    },
    /// Sent unasked.
    Event { event: ProviderEvent },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOutcome {
    Response(ProviderResponse),
    /// `VersionMismatch` for a hello naming no version the adapter speaks, or a request before hello. `Conflict`
    /// for another hub's hello, or a request the instance's state refuses.
    /// `NotFound` for an instance that isn't there, `Unavailable` when the provider has no room, such as no spot
    /// instances, `Unsupported` for what the adapter doesn't do or can't read, and `Worker` for the rest.
    Error(ApiError),
}

/// What an adapter tells its hub without being asked, so the hub needn't find out when a worker's stream breaks.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderEvent {
    /// It's about to end, such as a spot instance the cloud is taking back, at `at_ms` if the provider says when.
    InstanceEnding {
        instance_id: InstanceId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at_ms: Option<i64>,
        reason: String,
    },
    /// It's gone, such as with the host it ran on.
    InstanceLost { instance_id: InstanceId, reason: String },
    #[serde(other)]
    Unknown,
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
