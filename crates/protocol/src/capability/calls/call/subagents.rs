//! Calls that hand work to subagents, and talk to them while they work.

use serde::{Deserialize, Serialize};

use super::Runs;
use crate::capability::settings::{Effort, PersonaName};
use crate::model::{ModelName, ToolUseId};

/// Handing work to a subagent. What it can do besides is what its call says: its own model or effort, or working on
/// in the background.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StartSubagent {
    /// The persona it runs as, such as `Explore`, or `None` for the agent's default one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<PersonaName>,
    /// What it's for, in a few words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub prompt: String,
    /// When the agent chose one for it, as the agent named it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    #[serde(default)]
    pub runs: Runs,
    /// The subagent it carries on with, by the call that started it, rather than starting a new one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumes: Option<ToolUseId>,
}

/// Telling a running subagent something, by the call that started it, as Codex's `send_input` and Claude's
/// `SendMessage` do.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MessageSubagent {
    pub subagent: ToolUseId,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WaitForSubagents {
    /// Empty waits for any of them.
    pub subagents: Vec<ToolUseId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StopSubagent {
    pub subagent: ToolUseId,
}
