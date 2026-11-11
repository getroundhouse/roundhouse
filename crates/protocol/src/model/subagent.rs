//! A subagent's record: what it was asked, where it's got to, and what it reported, which the hub keeps as it records
//! the events that say, so a client can show a subagent without reading its work.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Rev, SessionId, ToolUseId};
use crate::event::SubagentOutcome;

/// One subagent, named by its session and the call that started it, which its events name as their parent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Subagent {
    pub session_id: SessionId,
    pub call: ToolUseId,
    /// Its kind of agent, such as `Explore`.
    #[serde(default)]
    pub kind: Option<String>,
    /// What it's for, in a few words.
    #[serde(default)]
    pub description: Option<String>,
    /// What it was asked.
    #[serde(default)]
    pub prompt: Option<String>,
    /// It works on while the agent carries on, rather than its call waiting for it.
    #[serde(default)]
    pub in_background: bool,
    pub status: SubagentStatus,
    pub started_at_ms: i64,
    #[serde(default)]
    pub ended_at_ms: Option<i64>,
    /// How many calls it's made.
    #[serde(default)]
    pub calls: u32,
    /// The newest of them.
    #[serde(default)]
    pub latest_call: Option<Call>,
    /// What it reported, once it finished.
    #[serde(default)]
    pub report: Option<String>,
    #[serde(default)]
    pub rev: Rev,
}

/// Where a subagent has got to. One this build doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    Working,
    Finished,
    Failed,
    /// The agent stopped it, or it went with the agent.
    Stopped,
    #[serde(other)]
    Unknown,
}

impl SubagentStatus {
    /// How it is once it ended with `outcome`.
    pub fn ended(outcome: SubagentOutcome) -> Self {
        match outcome {
            SubagentOutcome::Finished => SubagentStatus::Finished,
            SubagentOutcome::Failed => SubagentStatus::Failed,
            SubagentOutcome::Stopped => SubagentStatus::Stopped,
            SubagentOutcome::Unknown => SubagentStatus::Unknown,
        }
    }
}

/// A call an agent or subagent made: which tool, with what.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Call {
    pub id: ToolUseId,
    pub name: String,
    pub input: Value,
}
