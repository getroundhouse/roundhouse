//! A session's settings, as one value, for whatever sets or launches with them: the model, the approval mode and the
//! effort. Requests flatten it, so on the wire each setting is a field of its own, as it always was.

use serde::{Deserialize, Serialize};

use super::Effort;

/// Each setting is `None` where a change leaves it as it is, or, for what an agent launches with, where it's left to
/// the user's own agent settings.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionOptions {
    pub model: Option<String>,
    /// Passed through to the agent.
    pub permission_mode: Option<String>,
    /// A peer from before effort sends none.
    #[serde(default)]
    pub effort: Option<Effort>,
}

impl SessionOptions {
    /// `newer`'s settings, and this one's where `newer` leaves one alone.
    #[must_use]
    pub fn merged(self, newer: SessionOptions) -> SessionOptions {
        SessionOptions {
            model: newer.model.or(self.model),
            permission_mode: newer.permission_mode.or(self.permission_mode),
            effort: newer.effort.or(self.effort),
        }
    }

    /// Whether it changes nothing.
    pub fn is_empty(&self) -> bool {
        self.model.is_none() && self.permission_mode.is_none() && self.effort.is_none()
    }
}

#[cfg(test)]
mod tests;
