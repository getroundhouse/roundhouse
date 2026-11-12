//! The agent's background tasks: the CLI's list of them, which of them are monitors, the call that started each, and
//! which left the list stopped rather than finishing.

use std::collections::{BTreeMap, BTreeSet};

use protocol::event::{AgentTask, EventBody, TaskKind};
use protocol::model::{TaskId, ToolUseId};
use protocol::worker::WAIT_TOOL;
use serde_json::Value;

use super::calls::OpenCalls;
use super::{Parsed, id_of, other, str_of, string_of};
use crate::claude::TOOLS_SERVER;

#[cfg(test)]
mod tests;

/// The tool whose task watches for something, rather than doing work that ends.
const MONITOR: &str = "Monitor";
/// The `task_type` of a subagent's task.
const SUBAGENT: &str = "local_agent";
/// The `task_type` of a task that a call to an MCP server's tool left running, the server having answered it with one.
const MCP_TASK: &str = "mcp_task";

/// `background_tasks_changed`, which is the whole list each time. Which are monitors, and which left it stopped, come
/// from what came before, which a [`super::Normalizer`] knows.
pub(super) fn listed(value: &Value) -> Parsed {
    match value.get("tasks").and_then(Value::as_array) {
        Some(tasks) => {
            Parsed::Event(EventBody::AgentTasks { tasks: tasks.iter().filter_map(task).collect(), stopped: Vec::new() })
        }
        // Saying nothing is running when it can't tell would let the agent be stopped mid-task.
        None => other("system/background_tasks_changed"),
    }
}

fn task(value: &Value) -> Option<AgentTask> {
    let kind = match str_of(value, "task_type") {
        Some(SUBAGENT) => TaskKind::Subagent,
        Some(MCP_TASK) if is_a_wait(value) => TaskKind::Wait,
        // A monitor's is `local_bash` too, as a command's is.
        _ => TaskKind::Command,
    };
    Some(AgentTask { id: id_of(value, "task_id")?, description: string_of(value, "description"), kind, call: None })
}

/// Whether an MCP task is a wait on the pull request, which the CLI says by describing it as the tool whose call
/// started it, with its server, as `roundhouse/wait_for_pull_request`. The list says so itself, so a CLI started again
/// with `--resume`, which lists the task again before any prompt, says so too, though the call was another process's.
fn is_a_wait(value: &Value) -> bool {
    str_of(value, "description").and_then(|description| description.split_once('/')) == Some((TOOLS_SERVER, WAIT_TOOL))
}

/// What the lines before a list say about the tasks in it, which the list itself doesn't.
#[derive(Debug, Default)]
pub(super) struct Tasks {
    /// Those the CLI has said it stopped, whether the agent stopped them or they went with it, until a list leaves
    /// them out. It says so before the list.
    stopped: BTreeSet<TaskId>,
    /// The tasks a `Monitor` call started, each until a list that held it leaves it out. Its `task_notification`
    /// doesn't end it, since the CLI can list a task once more after saying it ended.
    monitors: BTreeSet<TaskId>,
    /// The call that started each task, from its `task_started`, until a list that held it leaves it out, as for a
    /// monitor, since the CLI can list a task once more after saying it ended. A task that ends unlisted, as a command
    /// or a subagent in the foreground does, is forgotten as it ends. [`super::subagents::Subagents`] keeps a
    /// subagent's call by its task too, but only until it ends, which is when it has to say which subagent ended.
    calls: BTreeMap<TaskId, ToolUseId>,
    /// The newest list, to say again once `task_started` says more of one of its tasks that changes what's shown.
    newest: Vec<AgentTask>,
}

impl Tasks {
    /// Notes what this line says of a task. The CLI lists a task that starts in the background one line before
    /// `task_started` names the call that started it, so that line is the first to say which call that was, and
    /// whether it's a monitor. For a monitor, and for a subagent, whose line opens its tab by its call, this returns
    /// the newest list to say again. A command's call changes nothing shown, so it waits for the next list.
    /// [`Tasks::listed`] gives a list its calls and kinds. `task_started` comes before its call's result, so the call
    /// is still among `calls`.
    pub(super) fn note(&mut self, value: &Value, calls: &OpenCalls) -> Option<EventBody> {
        match (str_of(value, "type"), str_of(value, "subtype")) {
            (Some("system"), Some("task_started")) => return self.note_started(value, calls),
            (Some("system"), Some("task_notification")) => {
                let id = id_of::<TaskId>(value, "task_id")?;
                if !self.newest.iter().any(|task| task.id == id) {
                    self.calls.remove(&id);
                }
                if str_of(value, "status") == Some("stopped") {
                    self.stopped.insert(id);
                }
            }
            _ => {}
        }
        None
    }

    fn note_started(&mut self, value: &Value, calls: &OpenCalls) -> Option<EventBody> {
        let call: ToolUseId = id_of(value, "tool_use_id")?;
        let id: TaskId = id_of(value, "task_id")?;
        let monitor = calls.tool(call.as_str()) == Some(MONITOR);
        if monitor {
            self.monitors.insert(id.clone());
        }
        let shown = monitor || str_of(value, "task_type") == Some(SUBAGENT);
        let already_listed = self.newest.iter().any(|task| task.id == id);
        self.calls.insert(id, call);
        (shown && already_listed).then(|| EventBody::AgentTasks { tasks: self.newest.clone(), stopped: Vec::new() })
    }

    /// Says which call started each of a list's tasks and which are monitors, and returns those the CLI said it
    /// stopped that the list leaves out, which it then forgets.
    pub(super) fn listed(&mut self, tasks: &mut [AgentTask]) -> Vec<TaskId> {
        let left = |id: &TaskId| self.newest.iter().any(|t| t.id == *id) && tasks.iter().all(|t| t.id != *id);
        self.monitors.retain(|id| !left(id));
        self.calls.retain(|id, _| !left(id));
        for task in tasks.iter_mut() {
            if self.monitors.contains(&task.id) {
                task.kind = TaskKind::Monitor;
            }
            task.call = self.calls.get(&task.id).cloned();
        }
        self.newest = tasks.to_vec();
        let (gone, still_listed): (BTreeSet<_>, _) =
            std::mem::take(&mut self.stopped).into_iter().partition(|id| tasks.iter().all(|t| t.id != *id));
        self.stopped = still_listed;
        gone.into_iter().collect()
    }
}
