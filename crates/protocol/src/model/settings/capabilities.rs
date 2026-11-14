//! The settings older clients and stored chats use, translated once at the capability boundary.

use crate::capability::{Capabilities, settings};
use crate::model::{Effort, NotOffered, PermissionMode, SessionSettings, SessionSettingsChange};

impl SessionSettings {
    pub fn for_capabilities(&self, claims: &Capabilities) -> Result<settings::Settings, NotOffered> {
        SessionSettingsChange {
            model: Some(self.model.clone()),
            permission_mode: Some(self.permission_mode),
            effort: Some(self.effort),
        }
        .for_capabilities(claims)
    }
}

impl SessionSettingsChange {
    /// The offered full-access or plan mode, with the adapter's sandbox and approval policy, rather than assumptions
    /// about how a CLI implements it. The persisted settings still read exactly as older chats did.
    pub fn for_capabilities(&self, claims: &Capabilities) -> Result<settings::Settings, NotOffered> {
        let mode = self
            .permission_mode
            .map(|mode| {
                let modes = claims.approvals.modes.as_ref().ok_or(NotOffered::Mode)?;
                if mode == PermissionMode::Unknown {
                    return Err(NotOffered::Mode);
                }
                std::iter::once(modes.default)
                    .chain(modes.others.iter().copied())
                    .find(|offered| offered.is_known() && offered.plan == (mode == PermissionMode::Plan))
                    .ok_or(NotOffered::Mode)
            })
            .transpose()?;
        if self.effort == Some(Effort::Unknown) {
            return Err(NotOffered::Effort);
        }
        Ok(settings::Settings {
            model: self.model.clone(),
            mode,
            effort: self.effort.map(Into::into),
            ..Default::default()
        })
    }
}

impl From<Effort> for settings::Effort {
    fn from(effort: Effort) -> Self {
        match effort {
            Effort::Low => Self::Low,
            Effort::Medium => Self::Medium,
            Effort::High => Self::High,
            Effort::Xhigh => Self::Xhigh,
            Effort::Max => Self::Max,
            Effort::Ultra => Self::Ultra,
            Effort::Unknown => Self::Unknown,
        }
    }
}
