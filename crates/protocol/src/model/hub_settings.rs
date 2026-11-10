//! The settings a hub carries out itself, whichever client chose them, so it keeps them and every client shows the
//! same.

use serde::{Deserialize, Serialize};

use super::Rev;

/// A hub's settings. There's one record of them per hub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubSettings {
    /// Whether it keeps its machine from going to sleep by itself while there's work.
    pub keep_awake: bool,
    pub rev: Rev,
}

/// Settings to change. One left out is left as it is.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct HubSettingsChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_awake: Option<bool>,
}
