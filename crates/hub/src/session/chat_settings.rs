//! What a chat runs with, as its record keeps it: a model, an approval mode and an effort, each always set.

use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings::{Effort, Settings};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ModelName, Session};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChatSettings {
    pub(crate) model: ModelName,
    pub(crate) mode: ApprovalMode,
    pub(crate) effort: Effort,
}

impl ChatSettings {
    /// What `session`'s record says it runs with.
    pub(crate) fn of(session: &Session) -> Self {
        ChatSettings { model: session.model.clone(), mode: session.mode, effort: session.effort.clone() }
    }

    /// `settings`, which name all three, or a refusal of what names fewer, such as a kind's defaults that leave its
    /// mode to its claims and name no mode its claims give.
    pub(crate) fn named(settings: Settings) -> Result<Self, ApiError> {
        let left_out = |what| ApiError::new(ErrorCode::InvalidRequest, format!("the chat's settings name no {what}"));
        Ok(ChatSettings {
            model: settings.model.ok_or_else(|| left_out("model"))?,
            mode: settings.mode.ok_or_else(|| left_out("approval mode"))?,
            effort: settings.effort.ok_or_else(|| left_out("effort"))?,
        })
    }

    /// Them, as a launch or a check of what a kind offers takes them.
    pub(crate) fn settings(&self) -> Settings {
        Settings {
            model: Some(self.model.clone()),
            mode: Some(self.mode),
            effort: Some(self.effort.clone()),
            ..Settings::default()
        }
    }

    /// Them, with what `change` sets in place of what they were. `change` sets nothing else, which
    /// [`ChatSettings::keeps`] says.
    #[must_use]
    pub(crate) fn changed(self, change: &Settings) -> Self {
        ChatSettings {
            model: change.model.clone().unwrap_or(self.model),
            mode: change.mode.unwrap_or(self.mode),
            effort: change.effort.clone().unwrap_or(self.effort),
        }
    }

    /// Refuses a change of any setting but the model, the mode and the effort, which a chat's record doesn't keep.
    pub(crate) fn keeps(change: &Settings) -> Result<(), ApiError> {
        let Settings { model: _, mode: _, effort: _, speed, thinking, persona, style, verbosity, advisor, budget } =
            change;
        let others = speed.is_some()
            || thinking.is_some()
            || persona.is_some()
            || style.is_some()
            || verbosity.is_some()
            || advisor.is_some()
            || budget.is_some();
        if others {
            let message = "a chat keeps only its model, approval mode and effort";
            return Err(ApiError::new(ErrorCode::InvalidRequest, message));
        }
        Ok(())
    }
}
