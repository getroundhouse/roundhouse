//! Work that runs beside the agent's own: subagents, and tasks that carry on after the turn that started them.

use serde::{Deserialize, Serialize};

use super::Claimed;
use super::area::area;

area! { "background work";
    /// The agent hands work to subagents. Each is reported as it starts, as its context grows and as it ends, its
    /// events are recorded as its own by the call that started it, and its requests for approval come through the
    /// agent's, whether the agent sends a subagent's work in its own stream, as Claude does, or as a conversation of
    /// its own, as Codex and opencode do. What else a subagent does, such as run on its own model or in the
    /// background, is what its call says.
    Subagents => subagents: Claimed,
    /// A prompt can go to a subagent of a named persona directly, without the agent choosing to, as opencode's
    /// subtasks do, as the prompt's [`Extras::subagent`](super::prompts::Extras::subagent).
    DirectSubagents => direct_subagents: Claimed,
    /// Commands the agent leaves running carry on after its turn, each listed whenever the list changes, until it ends
    /// or is stopped. The agent hears how each ended as [`BackgroundCommands::hears`] says. A command that watches for
    /// something and wakes the agent each time is one, marked as a monitor.
    BackgroundCommands => background_commands: BackgroundCommands,
    /// A tool call can carry on in the background as a task of its own, such as a wait the hub keeps, and the agent
    /// hears how it ended, as Claude does with MCP's tasks.
    ToolTasks => tool_tasks: Claimed,
    /// Any background task can be stopped, and the list says it was stopped rather than finishing.
    StopTasks => stop_tasks: Claimed,
    /// What a background task has printed or done so far can be read while it runs, and once it's done, as the answer
    /// to the hub's asking, which nothing keeps.
    TaskOutput => task_output: Claimed,
    /// The agent can set a prompt to send itself later, once or on a schedule, with a [`Call::Schedule`], for as long
    /// as its process runs. Each is listed as a task of a kind of its own until it's sent, and stopping the task drops
    /// it. It goes when the process ends, and the adapter ends its task then, so the list never shows one that won't
    /// come, and the hub doesn't start the agent for one. A schedule the agent keeps past its process, as Claude's
    /// `CronCreate` can with `durable`, isn't part of this, as the `capability` module says.
    ///
    /// [`Call::Schedule`]: super::calls::Call::Schedule
    Schedules => schedules: Claimed,
    /// The agent can run a workflow, a script that runs subagents of its own, as a background task listed until it
    /// ends, with a [`Call::Workflow`], as Claude's `Workflow` does.
    ///
    /// [`Call::Workflow`]: super::calls::Call::Workflow
    Workflows => workflows: Claimed,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackgroundCommands {
    pub hears: Hears,
}

/// How the agent hears that a command it left running ended, which a client says beside the command.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Hears {
    /// In a turn of its own, which it starts by itself, as Claude's does.
    AtOnce,
    /// Only when it next looks, in a turn something else started, as Codex's does.
    WhenItLooks,
    #[serde(other)]
    Unknown,
}
