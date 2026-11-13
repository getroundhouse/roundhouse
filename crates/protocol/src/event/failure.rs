//! One of the agent's requests to its model failing, as its CLI said.

use serde::{Deserialize, Serialize};

/// Why one of the agent's requests to its model failed, as the agent's CLI said in a message of its own, in the place
/// of the model's reply.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RequestFailure {
    /// What went wrong, in the CLI's own words, which say what to do about it, such as choosing another model. It can
    /// hold Markdown, as the agent's words can.
    pub message: String,
    #[serde(default)]
    pub kind: FailureKind,
    /// When the limit that refused the request starts again, for a limit, when the CLI says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at_ms: Option<i64>,
}

/// What kind of failure it was, which says what would get past it. The agent's adapter reads it from what the CLI says
/// of the failure besides its words, since its words can change. One this build doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// A limit on how much the account can use was reached.
    Limit,
    /// The agent's login was refused, or it has none.
    SignIn,
    /// Something about the account stands in the way, such as its billing, or a hold on it.
    Account,
    /// The model's API is too busy to answer.
    Overloaded,
    /// The model's API failed.
    ServerError,
    /// The model's API refused what it was sent, such as a model it doesn't have.
    Refused,
    /// The CLI didn't say which, or said something this build doesn't know.
    #[default]
    #[serde(other)]
    Unknown,
}
