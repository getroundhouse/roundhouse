//! The settings a hub carries out itself, whichever client chose them, so it keeps them and every client shows the
//! same.

use serde::{Deserialize, Serialize};

use super::{AccountId, AgentKind, Rev};

/// A hub's settings. There's one record of them per hub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubSettings {
    /// Whether it keeps its machine from going to sleep by itself while there's work.
    pub keep_awake: bool,
    /// Whether it allows a tool's use itself when an agent with full access asks anyway, as Claude's CLI does before
    /// a command it can't check, so nobody waits on it. `None` from a hub from before it could.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_asked_in_full_access: Option<bool>,
    /// Defaults chosen on this hub, by agent kind. None means the hub predates account defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_defaults: Option<Vec<AccountDefault>>,
    pub rev: Rev,
}

/// Settings to change. One left out is left as it is.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct HubSettingsChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_awake: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_asked_in_full_access: Option<bool>,
}

/// The account new processes of one agent kind run under. Running processes keep their login.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountDefault {
    pub agent: AgentKind,
    pub account: AccountId,
}
