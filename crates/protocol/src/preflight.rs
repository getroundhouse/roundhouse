//! One read-only question about a prospective launch. The hub writes a single newline-delimited [`Asked`]
//! to the route opened by [`crate::provider::OpenPreflight`] and keeps stdin open until the reply arrives.
//! Closing stdin cancels the exchange.
//!
//! There's no hello, since the stream carries one question. A question or a field is added as the other protocols
//! add them, and [`VERSION`] goes up only for a break, which an endpoint that doesn't speak it refuses.

use crate::model::AgentLogin;
use serde::{Deserialize, Serialize};

/// The version of the exchange this build writes and reads.
pub const VERSION: u32 = 1;

/// What the hub writes.
#[derive(Serialize, Deserialize, Debug)]
pub struct Asked {
    /// The oldest version that has what's asked. An endpoint answers one newer than its own with
    /// [`crate::client::ErrorCode::Unsupported`].
    pub version: u32,
    #[serde(flatten)]
    pub question: Question,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "question", rename_all = "snake_case")]
pub enum Question {
    /// Replies with [`crate::client::Outcome`] of [`crate::capability::settings::ModelCatalog`].
    Models { login: Option<AgentLogin> },
    /// A newer hub's question. The endpoint answers with [`crate::client::ErrorCode::Unsupported`].
    #[serde(other)]
    Unknown,
}
