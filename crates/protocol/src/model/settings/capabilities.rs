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

impl SessionSettings {
    /// What a launch's typed `settings` say, in the form an adapter that still runs its agent with these takes, with
    /// what they leave out taken from `defaults`, its kind's. A launch names its model, so one without is refused, and
    /// so is a mode or an effort this build has no name for here, which the adapter couldn't tell its agent. Only
    /// `agents::registry` uses it, until the adapters take typed settings in #1274.
    pub fn from_typed(
        settings: &settings::Settings,
        defaults: &SessionSettings,
    ) -> Result<SessionSettings, NotOffered> {
        let model = settings.model.clone().ok_or(NotOffered::Settings)?;
        let permission_mode = match settings.mode {
            None => defaults.permission_mode,
            Some(mode) if !mode.is_known() => return Err(NotOffered::Mode),
            Some(mode) if mode.plan => PermissionMode::Plan,
            Some(_) => PermissionMode::FullAccess,
        };
        let effort = match settings.effort {
            None => defaults.effort,
            Some(effort) => Effort::ALL
                .into_iter()
                .find(|named| settings::Effort::from(*named) == effort && effort != settings::Effort::Unknown)
                .ok_or(NotOffered::Effort)?,
        };
        Ok(SessionSettings { model, permission_mode, effort })
    }
}
