//! The agent's background tasks: the CLI's list of them, and which left it stopped rather than finishing.

use std::collections::BTreeSet;

use protocol::event::{AgentTask, EventBody};
use protocol::model::TaskId;
use serde_json::Value;

use super::{Parsed, id_of, other, str_of, string_of};

/// `background_tasks_changed`, which is the whole list each time. Which left it stopped comes from what came before,
/// which a [`super::Normalizer`] knows.
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
    Some(AgentTask { id: id_of(value, "task_id")?, description: string_of(value, "description") })
}

/// Background tasks the CLI has said it stopped, whether the agent stopped them or they went with it, until a list
/// leaves them out. It says so before the list.
#[derive(Debug, Default)]
pub(super) struct StoppedTasks(BTreeSet<TaskId>);

impl StoppedTasks {
    /// Notes a task stopped, if this line says one was.
    pub(super) fn note(&mut self, value: &Value) {
        let notice = str_of(value, "type") == Some("system") && str_of(value, "subtype") == Some("task_notification");
        if notice
            && str_of(value, "status") == Some("stopped")
            && let Some(id) = id_of(value, "task_id")
        {
            self.0.insert(id);
        }
    }

    /// Those it said it stopped that this list leaves out, which it then forgets.
    pub(super) fn gone_from(&mut self, listed: &[AgentTask]) -> Vec<TaskId> {
        let (gone, still_listed): (BTreeSet<_>, _) =
            std::mem::take(&mut self.0).into_iter().partition(|id| listed.iter().all(|t| t.id != *id));
        self.0 = still_listed;
        gone.into_iter().collect()
    }
}
