//! What a kind of agent is called and offers, as its adapter describes it.

#[cfg(test)]
use super::{AgentKind, Effort, ModelName, PermissionMode, SessionSettings, SessionSettingsChange};

#[cfg(test)]
mod tests;

mod description;
pub use description::{AgentDescription, AgentFeature, LoginDescription, ModelChoice, NotOffered};
