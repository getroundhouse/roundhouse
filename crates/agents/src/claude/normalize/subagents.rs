//! The CLI's word on its subagents. Each runs as one of its tasks: `task_started` says one started, with what it was
//! asked, and `task_notification` says how it ended, with its report.

use std::collections::{HashMap, HashSet};

use protocol::event::{EventBody, SubagentOutcome};
use protocol::model::{TaskId, ToolUseId};
use serde_json::Value;

use super::{id_of, str_of, string_of};

/// The subagents the CLI has started and not said have ended, by the task each runs as. Its word that a task ended
/// doesn't say whether the task was a subagent, and doesn't always name the call.
#[derive(Debug, Default)]
pub(super) struct Subagents {
    by_task: HashMap<TaskId, ToolUseId>,
    /// By the call that started each, those in the background, which carry on after the turn ends.
    in_background: HashSet<ToolUseId>,
}

impl Subagents {
    /// Whether the subagent `call` started works in the background.
    pub(super) fn in_background(&self, call: &ToolUseId) -> bool {
        self.in_background.contains(call)
    }

    /// The event this line makes, if it says a subagent started or ended.
    pub(super) fn read(&mut self, value: &Value) -> Option<EventBody> {
        if str_of(value, "type") != Some("system") {
            return None;
        }
        match str_of(value, "subtype")? {
            // Other tasks, such as a command left running, have another `task_type`.
            "task_started" if str_of(value, "task_type") == Some("local_agent") => {
                let call: ToolUseId = id_of(value, "tool_use_id")?;
                if let Some(task) = id_of(value, "task_id") {
                    self.by_task.insert(task, call.clone());
                }
                let in_background = value.get("is_backgrounded").and_then(Value::as_bool) == Some(true);
                if in_background {
                    self.in_background.insert(call.clone());
                }
                Some(EventBody::SubagentStarted {
                    call,
                    kind: string_of(value, "subagent_type"),
                    description: string_of(value, "description"),
                    prompt: string_of(value, "prompt"),
                    in_background,
                })
            }
            "task_notification" => {
                let call = self.by_task.remove(&id_of::<TaskId>(value, "task_id")?)?;
                self.in_background.remove(&call);
                let outcome = match str_of(value, "status") {
                    Some("completed") => SubagentOutcome::Finished,
                    Some("failed") => SubagentOutcome::Failed,
                    Some("stopped") => SubagentOutcome::Stopped,
                    _ => SubagentOutcome::Unknown,
                };
                // Stopped, it only says so.
                let report = string_of(value, "summary").filter(|_| outcome == SubagentOutcome::Finished);
                Some(EventBody::SubagentEnded { call, outcome, report })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
