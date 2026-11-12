use std::io;

use protocol::client::{ApiError, ErrorCode, error_chain};
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
    /// What an adapter in another process answered with, as the provider protocol carries it.
    #[error("{}", .0.message)]
    Adapter(ApiError),
}

impl ProviderError {
    pub fn io(what: impl Into<String>, source: io::Error) -> Self {
        Self::Io { what: what.into(), source }
    }

    /// As the provider protocol carries it. The hub gives a client a code of its own for it.
    pub fn to_api_error(&self) -> ApiError {
        let code = match self {
            Self::NotFound(_) => ErrorCode::NotFound,
            Self::Unsupported(_) => ErrorCode::Unsupported,
            Self::Io { .. } => ErrorCode::Worker,
            Self::Adapter(e) => return e.clone(),
        };
        ApiError::new(code, error_chain(self))
    }
}
