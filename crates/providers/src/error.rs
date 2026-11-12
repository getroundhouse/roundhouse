use std::io;

use protocol::model::InstanceId;

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("instance {0} doesn't exist")]
    NotFound(InstanceId),
    /// What this provider doesn't do, such as making a base image.
    #[error("this provider can't {0}")]
    Unsupported(&'static str),
    /// What it was doing, and what went wrong.
    #[error("{what}")]
    Io { what: String, source: io::Error },
}

impl ProviderError {
    pub(crate) fn io(what: impl Into<String>, source: io::Error) -> Self {
        Self::Io { what: what.into(), source }
    }
}
