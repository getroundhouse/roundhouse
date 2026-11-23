//! What the hub observes of an agent's account, which its provider owns.

use serde::{Deserialize, Serialize};

/// Who an agent's account is, as a worker last read it from the agent's login, each a person reads and none the hub
/// decides by. Each read replaces it whole, so one that no longer says, as a plan that's gone, says none. Any can be
/// `None`, where the agent's CLI doesn't say.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountIdentity {
    /// Who serves the model, such as `Anthropic`, or a cloud's API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    /// The account's plan, such as `max`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// How it's logged in, such as `Claude account`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}
