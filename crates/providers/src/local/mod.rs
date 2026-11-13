//! Runs workers on this machine. Each one is a separate process, which the hub reaches with the command `connect`
//! answers with, so a remote worker only changes the command. That's the daemon's own `worker --attach`, which starts
//! the worker in a session of its own if none is serving the instance, so the worker can outlive its hub.

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

pub use provider::{LocalConfig, LocalProvider};
#[cfg(test)]
use provider::{
    PATH_MARKER, merge_paths, parse_marked_path, read_login_path, read_until_second_marker, stop_orphaned_agents,
};
