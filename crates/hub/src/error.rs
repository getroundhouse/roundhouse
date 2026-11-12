use std::io;

use protocol::client::{ApiError, ErrorCode};
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

/// What a client gets for a provider's failure, whatever code the provider gave: `Unavailable` for what it can't do
/// or has no room for, and `Worker` for the rest. A client would read the provider protocol's `NotFound` as a file that
/// has gone, and its `Unsupported` as a hub older than the client.
pub(crate) fn provider_failed(error: &ProviderError) -> ApiError {
    let ApiError { code, message } = error.to_api_error();
    let unavailable = matches!(code, ErrorCode::Unsupported | ErrorCode::Unavailable);
    ApiError::new(if unavailable { ErrorCode::Unavailable } else { ErrorCode::Worker }, message)
}
