use super::super::tasks::outcome;
use super::super::{id_of, str_of, string_of};
use protocol::event::{EventBody, TaskOutcome};
use protocol::model::{SubagentId, TaskId, ToolUseId};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The subagents the CLI has started and not said have ended, by the task each runs as. Its word that a task ended
/// doesn't say whether the task was a subagent, and doesn't always name the call. Each is named by the task it runs
/// as, which its lines name as `agent_id`, beside the call that started it as their parent, and which outlives that
/// call: the agent carries on with one by it, and the CLI starts its task again when it does, under the call that
/// carries on.
#[derive(Debug, Default)]
pub(in super::super) struct Subagents {
    /// The subagent each task is, until it ends. [`super::super::tasks::Tasks`] keeps its call until the list leaves
    /// its task out, which can be after it ends.
    pub(super) by_task: HashMap<TaskId, SubagentId>,
    /// The subagent each call started, for a line that names only its parent, as a streamed message may. Every
    /// subagent's other lines named the task too, on 2.1.319 to 2.1.325, as `docs/claude-cli.md` says.
    pub(super) by_call: HashMap<ToolUseId, SubagentId>,
    /// Those in the background, which carry on after the turn ends.
    pub(super) in_background: HashSet<SubagentId>,
}

impl Subagents {
    /// Whether `subagent` works in the background.
    pub(in super::super) fn in_background(&self, subagent: &SubagentId) -> bool {
        self.in_background.contains(subagent)
    }

    /// The subagent whose request for approval this line is, or `None` for the agent's own. A request names the task
    /// its subagent runs as, as `agent_id`, which is the subagent's ID, and nothing else of it.
    pub(in super::super) fn asked_by(value: &Value) -> Option<SubagentId> {
        id_of(value.get("request")?, "agent_id")
    }

    /// The subagent whose line this is, or `None` for the agent's own: the one it names as `agent_id`, or else the one
    /// its parent started, as the CLI said. One whose start wasn't heard, such as before the adapter started again, is
    /// named by its call.
    pub(in super::super) fn of_line(&self, value: &Value) -> Option<SubagentId> {
        let call: ToolUseId = id_of(value, "parent_tool_use_id")?;
        let named = id_of(value, "agent_id").or_else(|| self.by_call.get(&call).cloned());
        Some(named.unwrap_or_else(|| SubagentId::of_call(&call)))
    }

    /// The event this line makes, if it says a subagent started or ended, or how much its conversation holds.
    pub(in super::super) fn read(&mut self, value: &Value) -> Option<EventBody> {
        if str_of(value, "type") != Some("system") {
            return None;
        }
        match str_of(value, "subtype")? {
            // Other tasks, such as a command left running, have another `task_type`.
            "task_started" if str_of(value, "task_type") == Some("local_agent") => {
                let call: ToolUseId = id_of(value, "tool_use_id")?;
                let task: Option<TaskId> = id_of(value, "task_id");
                let subagent = task.as_ref().map_or_else(|| SubagentId::of_call(&call), |task| task.as_str().into());
                if let Some(task) = task {
                    self.by_task.insert(task, subagent.clone());
                }
                self.by_call.insert(call.clone(), subagent.clone());
                let in_background = value.get("is_backgrounded").and_then(Value::as_bool) == Some(true);
                if in_background {
                    self.in_background.insert(subagent.clone());
                }
                Some(EventBody::SubagentStarted {
                    call,
                    subagent: Some(subagent),
                    kind: string_of(value, "subagent_type"),
                    description: string_of(value, "description"),
                    prompt: string_of(value, "prompt"),
                    in_background,
                })
            }
            // Only a subagent's task is in `by_task`, so another's progress, such as a tool's, stays as it was, and so
            // does a line with no count.
            "task_progress" => {
                let subagent = self.by_task.get(&id_of::<TaskId>(value, "task_id")?)?.clone();
                Some(EventBody::SubagentContext { subagent, tokens: context_tokens(value)? })
            }
            "task_notification" => {
                let subagent = self.by_task.remove(&id_of::<TaskId>(value, "task_id")?)?;
                self.in_background.remove(&subagent);
                let outcome = outcome(value);
                // Stopped, it only says so.
                let report = string_of(value, "summary").filter(|_| outcome == TaskOutcome::Finished);
                Some(EventBody::SubagentEnded { subagent, outcome, report, context_tokens: context_tokens(value) })
            }
            _ => None,
        }
    }
}

/// The CLI's count of tokens for the task this line is about, if it has one, which for a subagent's is about what its
/// conversation holds, as `docs/claude-cli.md` says.
fn context_tokens(value: &Value) -> Option<u64> {
    value.get("usage")?.get("total_tokens")?.as_u64()
}
