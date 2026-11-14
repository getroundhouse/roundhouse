use super::super::{Effort, ModelName, PermissionMode};
use serde::{Deserialize, Serialize};

/// What a session runs with. It always names a model, an approval mode and an effort, so the agent's CLI never picks
/// any of them for itself. What a session runs with until another is chosen is its agent's to say, as
/// [`AgentDescription::defaults`](super::super::AgentDescription::defaults) does.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SessionSettings {
    pub model: ModelName,
    pub permission_mode: PermissionMode,
    pub effort: Effort,
}

impl SessionSettings {
    /// These settings, with what `change` sets in place of what they were.
    #[must_use]
    pub fn changed(self, change: SessionSettingsChange) -> SessionSettings {
        SessionSettings {
            model: change.model.unwrap_or(self.model),
            permission_mode: change.permission_mode.unwrap_or(self.permission_mode),
            effort: change.effort.unwrap_or(self.effort),
        }
    }
}

/// Settings to change. One left out is left as it is.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionSettingsChange {
    pub model: Option<ModelName>,
    /// One the agent is offered, since the hub refuses any other, [`PermissionMode::Unknown`] included.
    pub permission_mode: Option<PermissionMode>,
    #[serde(default)]
    pub effort: Option<Effort>,
}

impl From<SessionSettings> for SessionSettingsChange {
    fn from(settings: SessionSettings) -> Self {
        Self {
            model: Some(settings.model),
            permission_mode: Some(settings.permission_mode),
            effort: Some(settings.effort),
        }
    }
}
