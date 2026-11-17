//! Work an agent started in the background, which carries on after the turn that started it.

use serde::{Deserialize, Serialize};

use super::TaskOutcome;
use crate::model::{EndedWait, SubagentId, TaskId, ToolUseId};

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
    /// The call that started it, or that started it again, once the agent has said. None from an agent or a hub that
    /// doesn't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<ToolUseId>,
    /// The subagent it is, by the ID its agent gives it, so its line opens the subagent tab on it, however it was
    /// started. None for any other kind, and from an agent or a hub that doesn't say, whose line opens the tab on the
    /// subagent its `call` started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent: Option<SubagentId>,
}

/// What a background task is, which says whether it's the agent's own work and whether it's doing anything.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// A subagent, which is the agent's own work.
    Subagent,
    /// A command that watches for something, and wakes the agent each time it happens. It does nothing meanwhile.
    Monitor,
    /// A wait on the workspace's pull request, which the hub keeps, so it goes on across the hub's restart, and the
    /// agent hears how it ended. It does nothing meanwhile but ask the hub how it stands. A client that doesn't know
    /// it reads it as a command.
    Wait,
    /// A command the agent left running. It might end by itself, as a test run does, or be held open, as an app is,
    /// and the agent doesn't say which. It's also what a list from before kinds were kept holds, and what a kind
    /// this build doesn't know reads as.
    #[default]
    #[serde(other)]
    Command,
}

/// A background task the agent heard had ended, as it started a turn by itself to take in how.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EndedTask {
    /// The task, as the agent's list of tasks last said it.
    pub task: AgentTask,
    pub outcome: TaskOutcome,
    /// For a wait on pull requests, what the hub kept of it: what it watched and how it ended. The hub adds it as it
    /// records the event, from its record of the wait the task's call started, since the agent hears only the tool's
    /// answer. `None` for any other kind, and from a hub that doesn't add it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<EndedWait>,
}
