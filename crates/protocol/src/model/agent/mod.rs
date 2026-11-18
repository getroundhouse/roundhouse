//! What a kind of agent is called and offers, as its adapter describes it.

#[cfg(test)]
use super::{AgentKind, ModelName, PermissionMode, SessionSettings, SessionSettingsChange};
#[cfg(test)]
use crate::capability::settings::Effort;

#[cfg(test)]
mod tests;

mod description;
pub use description::{AgentDescription, AgentFeature, LoginDescription, ModelChoice, NotOffered, RunCapabilities};
