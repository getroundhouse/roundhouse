//! Work an agent started in the background, which carries on after the turn that started it.

use serde::{Deserialize, Serialize};

use super::{EventBody, TaskOutcome};
use crate::model::{EndedWait, SubagentId, TaskId, ToolUseId};

/// Work an agent started in the background, such as a command or a subagent. It carries on after the turn that
/// started it ends.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentTask {
    pub id: TaskId,
    /// What the agent said it's for, in its own words. In a list recorded before tasks said their `command`, it can be
    /// the whole of the command a task runs, which Claude's CLI gives in place of a description its agent left out.
    #[serde(default)]
    pub description: Option<String>,
    /// The command it runs, whole, where the agent didn't say what that's for, so it can be named by the command. None
    /// where the agent said, and where the command isn't known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub kind: TaskKind,
    /// The call that started it, or that started it again, once the agent has said. None from an agent that doesn't
    /// say, and in a list recorded before calls were kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<ToolUseId>,
    /// The subagent it is, by the ID its agent gives it, so its line opens the subagent tab on it, however it was
    /// started. None for any other kind, from an agent that doesn't say, and in a list recorded before tasks named
    /// their subagents, whose line opens the tab on the subagent its `call` started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent: Option<SubagentId>,
    /// The subagent at work whose work it's part of, since that subagent's call started it. None for the agent's own,
    /// for one whose subagent has ended, which is then left with the agent, and from an agent that doesn't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_of: Option<SubagentId>,
}

impl AgentTask {
    /// One its agent has said nothing of but its ID and its kind.
    pub fn new(id: impl Into<TaskId>, kind: TaskKind) -> Self {
        Self { id: id.into(), description: None, command: None, kind, call: None, subagent: None, part_of: None }
    }
}

/// What a background task is, which says whether it's the agent's own work and whether it's doing anything.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// A subagent, which is the agent's own work.
    Subagent,
    /// A command that watches for something, and wakes the agent each time it happens. It does nothing meanwhile.
    Monitor,
    /// A wait on the workspace's pull request, which the hub keeps, so it goes on across the hub's restart, and the
    /// agent hears how it ended. It does nothing meanwhile but ask the hub how it stands.
    Wait,
    /// A command the agent left running. It might end by itself, as a test run does, or be held open, as an app is,
    /// and the agent doesn't say which. It's also what a kind this build doesn't know reads as, since it's the kind
    /// that claims least.
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
    /// answer. `None` for any other kind, and for a wait the hub has no record of, or couldn't read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<EndedWait>,
}

/// What the agent said of one of its background tasks, which [`EventBody::TaskChanged`](super::EventBody::TaskChanged)
/// records. One this build doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskChange {
    Started,
    /// How it's getting on, such as what a subagent has used so far.
    Progressed,
    /// A change to what was said of it, such as its moving to the background, or its status as it ends.
    Updated,
    Ended,
    #[serde(other)]
    Unknown,
}

impl EventBody {
    /// What the agent has running in the background once this is recorded, if it changes that: the list it gives, or
    /// nothing once it starts or exits, since what it ran goes with the agent that started it. `None` leaves what was
    /// running as it was. The hub keeps a session's list by this, and so its record's `background_tasks`.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only `AgentTasks`, `AgentStarted` and `AgentExited` change what runs in the background"
    )]
    pub fn sets_background(&self) -> Option<&[AgentTask]> {
        match self {
            EventBody::AgentTasks { tasks, .. } => Some(tasks),
            EventBody::AgentStarted { .. } | EventBody::AgentExited { .. } => Some(&[]),
            _ => None,
        }
    }
}
