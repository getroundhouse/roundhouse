//! The agent's background tasks: the CLI's list of them, which of them are monitors, and which left the list stopped
//! rather than finishing.

use std::collections::BTreeSet;

use protocol::event::{AgentTask, EventBody, TaskKind};
use protocol::model::TaskId;
use serde_json::Value;

use super::calls::OpenCalls;
use super::{Parsed, id_of, other, str_of, string_of};

#[cfg(test)]
mod tests;

/// The tool whose task watches for something, rather than doing work that ends.
const MONITOR: &str = "Monitor";

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
    // A monitor's is `local_bash` too, as a command's is.
    let kind = if str_of(value, "task_type") == Some("local_agent") { TaskKind::Subagent } else { TaskKind::Command };
    Some(AgentTask { id: id_of(value, "task_id")?, description: string_of(value, "description"), kind })
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
    /// The newest list, to say again once one of its tasks turns out to be a monitor.
    newest: Vec<AgentTask>,
}

impl Tasks {
    /// Notes what this line says of a task. The CLI lists a monitor's task one line before `task_started` names the
    /// call that started it, so that line is the first to say it's a monitor, and this returns the newest list to say
    /// again in its place. [`Tasks::listed`] gives it its kinds. `task_started` comes before its call's result, so
    /// the call is still among `calls`.
    pub(super) fn note(&mut self, value: &Value, calls: &OpenCalls) -> Option<Parsed> {
        match (str_of(value, "type"), str_of(value, "subtype")) {
            (Some("system"), Some("task_started")) => return self.note_started(value, calls),
            (Some("system"), Some("task_notification")) if str_of(value, "status") == Some("stopped") => {
                self.stopped.extend(id_of::<TaskId>(value, "task_id"));
            }
            _ => {}
        }
        None
    }

    fn note_started(&mut self, value: &Value, calls: &OpenCalls) -> Option<Parsed> {
        if calls.tool(str_of(value, "tool_use_id")?) != Some(MONITOR) {
            return None;
        }
        let id: TaskId = id_of(value, "task_id")?;
        let already_listed = self.newest.iter().any(|task| task.id == id);
        self.monitors.insert(id);
        already_listed.then(|| Parsed::Event(EventBody::AgentTasks { tasks: self.newest.clone(), stopped: Vec::new() }))
    }

    /// Says which of a list's tasks are monitors, and returns those the CLI said it stopped that the list leaves out,
    /// which it then forgets.
    pub(super) fn listed(&mut self, tasks: &mut [AgentTask]) -> Vec<TaskId> {
        let left = |id: &TaskId| self.newest.iter().any(|t| t.id == *id) && tasks.iter().all(|t| t.id != *id);
        self.monitors.retain(|id| !left(id));
        for task in tasks.iter_mut().filter(|task| self.monitors.contains(&task.id)) {
            task.kind = TaskKind::Monitor;
        }
        self.newest = tasks.to_vec();
        let (gone, still_listed): (BTreeSet<_>, _) =
            std::mem::take(&mut self.stopped).into_iter().partition(|id| tasks.iter().all(|t| t.id != *id));
        self.stopped = still_listed;
        gone.into_iter().collect()
    }
}
