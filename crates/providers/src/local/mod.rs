//! Runs workers on this machine. Each one is a separate process, the daemon's own `worker --ticket`, started in a
//! session of its own so it outlives its hub, which dials the hub at its endpoint on loopback, as every provider's
//! worker dials the hub. On Windows, where a worker doesn't dial yet, the hub runs `worker --stdio`, the command
//! `connect` answers with, and the worker stops with the hub.

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
