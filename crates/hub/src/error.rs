use std::io;

use protocol::client::{ApiError, ErrorCode, error_chain};
use providers::ProviderError;
use tracing::error;

use crate::store::StoreError;

/// Why a hub couldn't start or keep running. Failures of a single request are [`ApiError`]s.
#[derive(Debug, thiserror::Error)]
pub enum HubError {
    #[error("another hub is already running on {0}")]
    AlreadyRunning(String),
    #[error("opening the database")]
    OpenDatabase(#[source] StoreError),
    #[error("recovering from the last run")]
    Recover(#[source] StoreError),
    /// What it was doing, and what went wrong.
    #[error("{what}")]
    Io { what: String, source: io::Error },
}

impl HubError {
    pub(crate) fn io(what: impl Into<String>, source: io::Error) -> Self {
        Self::Io { what: what.into(), source }
    }
}

/// Something that shouldn't happen. The client only gets text, so it's logged here.
pub(crate) fn internal(message: impl Into<String>) -> ApiError {
    let message = message.into();
    error!("{message}");
    ApiError::new(ErrorCode::Internal, message)
}

/// Not a `From` impl, because `providers` doesn't depend on `protocol`, and neither type is the hub's. Each code is
/// one a client reads as a provider's failure. `NotFound` would read as a file that has gone, and `Unsupported` as a
/// hub older than the client.
pub(crate) fn provider_failed(error: &ProviderError) -> ApiError {
    let code = match error {
        ProviderError::NotFound(_) | ProviderError::Io { .. } => ErrorCode::Worker,
        ProviderError::Unsupported(_) => ErrorCode::Unavailable,
    };
    ApiError::new(code, error_chain(error))
}
