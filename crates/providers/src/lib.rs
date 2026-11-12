//! A provider gives a hub somewhere to run workers. Running on the same machine is one provider,
//! and nothing in the hub treats it as special.
//!
//! [`Provider`] is the provider protocol in `protocol::provider` as a Rust trait, one method for each request, with
//! the same rules: every request is safe to repeat, and `connect` answers with the command the hub runs to reach the
//! worker, so a provider is never in a worker's stream. "Providers" in `docs/architecture.md` has the design.

#![warn(clippy::wildcard_enum_match_arm)]

mod error;
pub mod local;
pub mod serve;

use async_trait::async_trait;
use protocol::model::InstanceId;
pub use protocol::provider::{Capabilities, Feature, Instance, InstanceState, Size, WorkerBinary, WorkerCommand};

pub use error::ProviderError;

/// Instance IDs are chosen by the hub, one per workspace.
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    /// Makes the instance, as a copy of the base image `from` or a plain one, or does nothing if it's there.
    async fn create(
        &self,
        instance_id: &InstanceId,
        from: Option<&InstanceId>,
        size: Option<Size>,
        display_name: &str,
    ) -> Result<(), ProviderError>;
    /// Starts it, or does nothing to one that's running, whatever runs there. First puts in place the one of `workers`
    /// built for the instance's target, if it holds a different one.
    async fn start(&self, instance_id: &InstanceId, workers: &[WorkerBinary]) -> Result<(), ProviderError>;
    /// Ends whatever runs on it, which can be a worker an earlier hub left, and leaves it awake. The hub has closed its
    /// own worker's stream and given it time to stop first. Safe on an instance that isn't running or doesn't exist.
    async fn stop(&self, instance_id: &InstanceId) -> Result<(), ProviderError>;
    /// Like `stop`, for a worker that isn't responding.
    async fn kill(&self, instance_id: &InstanceId) -> Result<(), ProviderError>;
    /// Safe on an instance that doesn't exist.
    async fn destroy(&self, instance_id: &InstanceId) -> Result<(), ProviderError>;
    async fn list(&self) -> Result<Vec<Instance>, ProviderError>;
    /// The command whose stdin and stdout are the worker's stream, which the hub runs. Can be asked again.
    async fn connect(&self, instance_id: &InstanceId) -> Result<WorkerCommand, ProviderError>;
    /// Turns a stopped instance into a base image. Refused by a provider without [`Feature::BaseImages`].
    async fn make_base_image(&self, instance_id: &InstanceId) -> Result<(), ProviderError>;
}
