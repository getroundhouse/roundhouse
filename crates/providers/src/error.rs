use std::io;

use protocol::model::InstanceId;

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("instance {0} is already running")]
    AlreadyRunning(InstanceId),
    #[error("instance {0} isn't running")]
    NotRunning(InstanceId),
    #[error("instance {0} already has a connection")]
    AlreadyConnected(InstanceId),
    #[error("worker {0} started without its pipes")]
    NoPipes(String),
    /// What it was doing, and what went wrong.
    #[error("{what}")]
    Io { what: String, source: io::Error },
}

impl ProviderError {
    pub(crate) fn io(what: impl Into<String>, source: io::Error) -> Self {
        Self::Io { what: what.into(), source }
    }
}
