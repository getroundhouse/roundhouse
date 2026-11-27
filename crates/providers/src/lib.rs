//! A provider gives a hub somewhere to run workers. Running on the same machine is one provider,
//! and nothing in the hub treats it as special.
//!
//! [`Provider`] is the provider protocol in `protocol::provider` as a Rust trait, one method for each request, with the
//! same rules: every request is safe to repeat, and a provider is never in a worker's stream. A worker a provider
//! starts dials the hub with its ticket, at the endpoint its [`WorkerReach`] says. The local provider is the one the
//! hub runs in itself, and it answers three more, which no adapter does: `workers_reach`, `connect`, the command the
//! hub runs to reach one of its workers that doesn't dial, and `workers_running`. "Workers and providers" in
//! `docs/principles/parts.md` has the rules a provider follows, and `docs/decisions/provider-adapters-over-stdio.md`
//! why an adapter is a program of its own.

#![warn(clippy::wildcard_enum_match_arm)]

mod error;
pub mod local;
pub mod serve;

use std::time::Duration;

use async_trait::async_trait;
use protocol::dialing::WorkerTicket;
use protocol::model::InstanceId;
pub use protocol::provider::{
    Capabilities, ClonePersistence, Feature, Instance, InstanceState, OpenPreflight, Size, WorkerBinary,
    WorkerEnvironment, WorkerFilesystem,
};

/// The command the hub runs to reach one of the local provider's workers, whose stdin and stdout are its stream.
pub type WorkerCommand = protocol::provider::Command;

/// How the workers a provider starts reach the hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerReach {
    /// They dial the hub's endpoint on its relays, which a worker on any machine can reach. Every adapter's.
    Relayed,
    /// They run on the hub's own machine and dial its endpoint on loopback, which uses no relay, so a hub whose workers
    /// are all this reaches nothing beyond loopback. The local provider's.
    Loopback,
    /// They don't dial: the hub runs the command `connect` gives, whose stdin and stdout are the worker's stream, and
    /// the worker stops with it. The local provider's on Windows, where a worker doesn't dial yet, and a test's.
    Command,
}

pub use error::ProviderError;

/// Instance IDs are chosen by the hub, one per workspace. [`serve`](serve::serve) gives up on a request that takes too
/// long, by dropping its future, so a provider ends whatever it started outside that future, such as a child process,
/// when the future is dropped, or a request after it, such as a `destroy`, can race what it left.
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    /// What the compute this provider's workers run on looks like to the hub.
    async fn environment(&self) -> Result<WorkerEnvironment, ProviderError>;
    /// The longest it takes to start one of its instances, up to running its worker there, or to stop one, with what
    /// runs there, so whatever waits for either can tell one that's stuck.
    fn starts_or_stops_within(&self) -> Duration;
    /// Makes the instance, as a copy of the base image `from` or a plain one, or does nothing if it's there. Its ID is
    /// the only name it's given.
    async fn create(
        &self,
        instance_id: &InstanceId,
        from: Option<&InstanceId>,
        size: Option<Size>,
    ) -> Result<(), ProviderError>;
    /// How the workers it starts reach the hub. Only the local provider's differ from an adapter's.
    fn workers_reach(&self) -> WorkerReach {
        WorkerReach::Relayed
    }
    /// Starts it, or does nothing to one that's running, whatever runs there. First puts in place the one of `workers`
    /// built for the instance's target, if it holds a different one. A worker it starts is given `ticket`, which it
    /// dials the hub with. The hub gives one to every provider whose workers dial, as [`Provider::workers_reach`]
    /// says.
    async fn start(
        &self,
        instance_id: &InstanceId,
        workers: &[WorkerBinary],
        ticket: Option<&WorkerTicket>,
    ) -> Result<(), ProviderError>;
    /// Ends whatever runs on it, which can be a worker an earlier hub left, and leaves it awake. The hub has closed its
    /// own worker's stream and given it time to stop first. Safe on an instance that isn't running or doesn't exist.
    async fn end_worker(&self, instance_id: &InstanceId) -> Result<(), ProviderError>;
    /// Like `end_worker`, for a worker that isn't responding.
    async fn kill_worker(&self, instance_id: &InstanceId) -> Result<(), ProviderError>;
    /// Safe on an instance that doesn't exist.
    async fn destroy(&self, instance_id: &InstanceId) -> Result<(), ProviderError>;
    async fn list(&self) -> Result<Vec<Instance>, ProviderError>;
    /// The command whose stdin and stdout are the worker's stream, which the hub runs. Can be asked again. Only the
    /// local provider's, for a worker that doesn't dial: one whose [`WorkerReach`] is `Command`, or one an earlier
    /// build left running, which listens.
    async fn connect(&self, _instance_id: &InstanceId) -> Result<WorkerCommand, ProviderError> {
        Err(ProviderError::Unsupported("reach a worker but by its dialing the hub"))
    }
    /// The instances a worker serves, which may be one an earlier hub left, so a hub that starts carries on with it.
    /// Only the local provider's, since the hub knows a worker that dials it by its dialing, and an earlier build's
    /// local workers listened instead.
    async fn workers_running(&self) -> Result<Vec<InstanceId>, ProviderError> {
        Err(ProviderError::Unsupported("say which workers run but by their dialing the hub"))
    }
    /// Opens a stream to run preflight in the prospective launch environment.
    async fn open_preflight(&self, _launch: &OpenPreflight) -> Result<protocol::provider::Command, ProviderError> {
        Err(ProviderError::Unsupported("open preflight"))
    }
    /// Turns a stopped instance into a base image. Refused by a provider without [`Feature::BaseImages`].
    async fn make_base_image(&self, instance_id: &InstanceId) -> Result<(), ProviderError>;
}
