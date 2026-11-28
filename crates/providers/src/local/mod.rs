//! Runs workers on this machine. Each one is a separate process, the daemon's own `worker --ticket`, started apart
//! from the hub so it outlives it, which dials the hub at its endpoint on loopback, as every provider's worker dials
//! the hub.

#[cfg(test)]
use crate::{Feature, InstanceState, Provider, ProviderError};
#[cfg(test)]
use process::ProcessRecord;
#[cfg(test)]
use protocol::model::InstanceId;
#[cfg(test)]
use protocol::paths::WORKER_RECORD_FILE;
#[cfg(test)]
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::time::Duration;

#[cfg(test)]
mod tests;

mod provider;
mod starting;

#[cfg(test)]
use provider::stop_orphans;
pub use provider::{LocalConfig, LocalProvider, end_worker_in};
