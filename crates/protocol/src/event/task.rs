//! Work an agent started in the background, which carries on after the turn that started it.

use serde::{Deserialize, Serialize};

use crate::model::TaskId;

/// Work an agent started in the background, such as a command or a subagent. It carries on after the turn that
/// started it ends.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentTask {
    pub id: TaskId,
    /// What the agent said it's for.
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub kind: TaskKind,
}

/// What a background task is, which says whether it's the agent's own work and whether it's doing anything.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// A subagent, which is the agent's own work.
    Subagent,
    /// A command that watches for something, and wakes the agent each time it happens. It does nothing meanwhile.
    Monitor,
    /// A command the agent left running. It might end by itself, as a test run does, or be held open, as an app is,
    /// and the agent doesn't say which. It's also what a list from before kinds were kept holds, and what a kind
    /// this build doesn't know reads as.
    #[default]
    #[serde(other)]
    Command,
}
