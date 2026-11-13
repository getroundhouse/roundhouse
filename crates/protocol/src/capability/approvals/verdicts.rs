//! What a reviewer decided of a use, and a use refused without asking.

use serde::{Deserialize, Serialize};

use crate::model::ToolUseId;

/// A reviewer's verdict on a request, as [`Capability::ReviewVerdicts`](super::Capability::ReviewVerdicts) reports it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub allowed: Allowed,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<Risk>,
    /// Why, in the reviewer's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Allowed {
    Yes,
    No,
    /// It took too long to decide, and the use didn't go ahead.
    TimedOut,
    #[serde(other)]
    Unknown,
}

/// How risky a reviewer judged a use. It has no order of its own, since one from a newer peer can't be placed.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Low,
    Medium,
    High,
    Critical,
    #[serde(other)]
    Unknown,
}

/// A use refused without asking, as [`Capability::Denials`](super::Capability::Denials) reports it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Denied {
    pub call: ToolUseId,
    pub by: DeniedBy,
    /// Why, in the agent's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeniedBy {
    /// The approval mode, which refuses what's beyond it.
    Mode,
    Rule,
    Reviewer,
    Hook,
    #[serde(other)]
    Unknown,
}
