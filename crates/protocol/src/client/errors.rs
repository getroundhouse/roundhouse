//! Why a hub, a worker or a provider didn't do what was asked, as each protocol's replies say it.

use serde::{Deserialize, Serialize};

use super::hello::HubRefusing;
use crate::model::TitleError;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    /// What a hub that refuses a hello for its versions says of itself. Any other error leaves it out, and so does a
    /// hub of a build from before it was said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub: Option<Box<HubRefusing>>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    LocalOnly,
    /// The first request wasn't `Hello`, or the versions don't match.
    VersionMismatch,
    NotFound,
    InvalidRequest,
    /// The request is valid but the current state doesn't allow it.
    Conflict,
    /// Asking again with `force` would go ahead, at the cost the message describes.
    NeedsForce,
    /// A name is in use, or the branch, folder of branches or directory it would need is.
    NameTaken,
    /// A worker or provider failed.
    Worker,
    /// What it needs isn't there, such as GitHub's CLI, a login to it, a GitHub remote, or what's left of GitHub's
    /// hourly limit, so asking again won't help until that changes.
    Unavailable,
    Internal,
    /// The hub, worker or provider adapter doesn't know the request, or can't read it, because it's older than
    /// whoever asked. The caller hides, disables or does without whatever needed it.
    Unsupported,
    #[serde(other)]
    Unknown,
}

/// An error and everything that caused it, on one line. [`ApiError`] only holds text, so this is what
/// keeps the cause when a lower error crosses a process.
pub fn error_chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), hub: None }
    }

    /// A hub's refusal of a hello that names no version it speaks, saying which hub it is.
    pub fn versions_differ(message: impl Into<String>, hub: HubRefusing) -> Self {
        Self { hub: Some(Box::new(hub)), ..Self::new(ErrorCode::VersionMismatch, message) }
    }

    /// The refusal of a request from a newer peer, which `reader`, such as "this hub", of the build whose version is
    /// `build`, doesn't know, or knows but can't read, such as one carrying a value it doesn't know. Its caller does
    /// without either way.
    pub fn unsupported_request(reader: &str, build: &str, unreadable: Option<&serde_json::Error>) -> Self {
        let message = match unreadable {
            None => format!("{reader}, version {build}, doesn't know that request"),
            Some(error) => format!("{reader}, version {build}, can't read that request: {error}"),
        };
        Self::new(ErrorCode::Unsupported, message)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

/// A title someone typed that the hub refuses, with a message they can read.
impl From<TitleError> for ApiError {
    fn from(e: TitleError) -> Self {
        ApiError::new(ErrorCode::InvalidRequest, e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_chain_names_every_cause_in_order() {
        #[derive(Debug)]
        struct Outer(std::io::Error);
        impl std::fmt::Display for Outer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("opening the log")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let error = Outer(std::io::Error::other("disk full"));

        assert_eq!(error_chain(&error), "opening the log: disk full");
    }
}
