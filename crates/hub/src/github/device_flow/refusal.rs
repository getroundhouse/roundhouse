use protocol::client::ApiError;
use serde::Deserialize;

use super::super::client::unavailable;

/// How GitHub's device flow says what went wrong, which it answers with a 200, so only `error` tells it from what was
/// asked for.
#[derive(Deserialize)]
pub(super) struct DeviceFlowError {
    /// Such as `authorization_pending`, `slow_down`, `expired_token` or `access_denied`.
    pub(super) error: String,
    #[serde(default)]
    pub(super) error_description: Option<String>,
    /// The time to keep between asks from then on, in seconds, which GitHub gives with `slow_down`.
    #[serde(default)]
    pub(super) interval: Option<u64>,
}

impl DeviceFlowError {
    /// What GitHub said, as a person reads it.
    pub(super) fn said(&self) -> String {
        match &self.error_description {
            Some(description) => format!("GitHub said {}: {description}", self.error),
            None => format!("GitHub said {}", self.error),
        }
    }
}

/// `Unavailable`, saying what GitHub answered when it was asked `what`, such as for a sign-in's code, with `body`
/// rather than what was asked for: its error, or that it's neither.
pub(super) fn refusal(what: &str, body: &str) -> ApiError {
    match serde_json::from_str::<DeviceFlowError>(body) {
        Ok(error) => unavailable(format!("asked {what}, {}", error.said())),
        Err(_) => unavailable(format!("GitHub didn't answer as asked {what}")),
    }
}
