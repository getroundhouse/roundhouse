//! What a session runs with, and changes to it: the model, the approval mode and the effort. Requests and launches
//! flatten them, so on the wire each setting is a field of its own.

use serde::{Deserialize, Serialize};

use super::{Effort, ModelName};

/// The model a session runs with until another is chosen. An alias, so it stays the newest of its kind without a release
/// here. `docs/spec/chats.md` has why it's this one.
pub const DEFAULT_MODEL: ModelName = ModelName::named("opus");

/// The effort a session runs with until another is chosen.
pub const DEFAULT_EFFORT: Effort = Effort::Xhigh;

/// What a session runs with. It always names a model and an effort, so the agent's CLI never picks either for itself.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SessionSettings {
    pub model: ModelName,
    /// Passed through to the agent. `None` leaves it to the user's own agent settings.
    pub permission_mode: Option<String>,
    pub effort: Effort,
}

impl Default for SessionSettings {
    /// What a session runs with until something else is chosen: [`DEFAULT_MODEL`] and [`DEFAULT_EFFORT`], in the user's
    /// own approval mode.
    fn default() -> Self {
        SessionSettings { model: DEFAULT_MODEL, permission_mode: None, effort: DEFAULT_EFFORT }
    }
}

impl SessionSettings {
    /// These settings, with what `change` sets in place of what they were.
    #[must_use]
    pub fn changed(self, change: SessionSettingsChange) -> SessionSettings {
        SessionSettings {
            model: change.model.unwrap_or(self.model),
            permission_mode: change.permission_mode.or(self.permission_mode),
            effort: change.effort.unwrap_or(self.effort),
        }
    }
}

/// Settings to change. One left out is left as it is.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionSettingsChange {
    pub model: Option<ModelName>,
    /// Passed through to the agent.
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub effort: Option<Effort>,
}

#[cfg(test)]
mod tests;
