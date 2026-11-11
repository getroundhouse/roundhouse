//! What became of a request for approval, and what the person decided.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalOutcome {
    Allowed,
    Denied,
    /// Nobody answered: the turn was interrupted, or the agent went away.
    Cancelled,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ApprovalDecision {
    Allow {
        /// Replaces the tool's input. `None` runs it as requested.
        #[serde(default)]
        updated_input: Option<Value>,
        /// Agent-defined rules to remember, taken from `suggestions`.
        #[serde(default)]
        updated_permissions: Option<Value>,
    },
    Deny {
        message: String,
        /// Also stops the turn rather than letting the agent try something else.
        #[serde(default)]
        interrupt: bool,
    },
    /// The agent asked the user something, and this is what they said, by the question's text. Several
    /// choices for one question are joined with a comma and a space.
    Answer { answers: BTreeMap<String, String> },
}
