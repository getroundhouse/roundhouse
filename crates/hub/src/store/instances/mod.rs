//! The hub's record of each instance it asks a provider for. It's written before the provider is asked, so the hub
//! knows which instances are its own whatever a provider lists, and can finish making or destroying one that a crash
//! cut short. Whether an instance exists, and whether it's running, is the provider's to say.

#[cfg(test)]
use super::Store;

#[cfg(test)]
mod tests;

mod instance_rows;
pub(super) use instance_rows::record_instance;
pub use instance_rows::{InstanceStage, RecordedInstance};
