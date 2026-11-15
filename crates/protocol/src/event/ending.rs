//! How what the log records came to an end: a turn, a task, and the agent itself.

use serde::{Deserialize, Serialize};

/// How a task stopped working: a subagent, or one of the agent's background tasks, such as a command it left running.
/// One this build doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskOutcome {
    Finished,
    Failed,
    /// The agent stopped it, or it went with the agent.
    Stopped,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Success,
    Cancelled,
    Error,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExitReason {
    /// The hub asked the agent to stop.
    Requested,
    /// The agent exited by itself.
    Unexpected,
    /// The worker process went away. The provider stops anything it left running.
    WorkerLost,
    /// The hub went away while the agent was running. Recorded when the hub next starts.
    HubRestart,
    /// The agent could not be started.
    StartFailed,
    #[serde(other)]
    Unknown,
}
