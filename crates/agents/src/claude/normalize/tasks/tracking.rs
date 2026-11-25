use super::super::calls::OpenCalls;
use super::super::subagents::Subagents;
use super::super::{Parsed, id_of, other, str_of, string_of};
use protocol::event::{AgentTask, EventBody, TaskChange, TaskKind, TaskOutcome};
use protocol::model::{SubagentId, TaskId, ToolUseId};
use protocol::worker::{TOOLS_SERVER, WAIT_TOOL, WAITS_TOOL};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The tool whose task watches for something, rather than doing work that ends.
const MONITOR: &str = "Monitor";
/// The `task_type` of a subagent's task.
const SUBAGENT: &str = "local_agent";
/// The `task_type` of a task that a call to an MCP server's tool left running, the server having answered it with one.
const MCP_TASK: &str = "mcp_task";

/// `background_tasks_changed`, which is the whole list each time. Which are monitors, and which left it stopped, come
/// from what came before, which a [`super::super::Normalizer`] knows.
pub(in super::super) fn listed(value: &Value) -> Parsed {
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
    let id: TaskId = id_of(value, "task_id")?;
    // A subagent is named by the task it runs as, as `super::super::subagents::Subagents` names it.
    let subagent = (kind == TaskKind::Subagent).then(|| id.as_str().into());
    Some(AgentTask { description: string_of(value, "description"), subagent, ..AgentTask::new(id, kind) })
}

/// Whether an MCP task is a wait on the pull request, which the CLI says by describing it as the tool whose call
/// started it, with its server, as `roundhouse/wait_for_pull_request`. The list says so itself, so a CLI started again
/// with `--resume`, which lists the task again before any prompt, says so too, though the call was another process's.
fn is_a_wait(value: &Value) -> bool {
    str_of(value, "description")
        .and_then(|description| description.split_once('/'))
        .is_some_and(|(server, tool)| server == TOOLS_SERVER && matches!(tool, WAIT_TOOL | WAITS_TOOL))
}

/// What the lines before a list say about the tasks in it, which the list itself doesn't.
#[derive(Debug, Default)]
pub(in super::super) struct Tasks {
    /// Those the CLI has said it stopped, whether the agent stopped them or they went with it, until a list leaves
    /// them out. It says so before the list.
    pub(super) stopped: BTreeSet<TaskId>,
    /// What each task's `task_started` said of it, until a list that held it leaves it out. Its `task_notification`
    /// doesn't end it, since the CLI can list a task once more after saying it ended. A task that ends unlisted, as a
    /// command or a subagent in the foreground does, is forgotten as it ends.
    /// [`super::super::subagents::Subagents`] keeps a subagent's call by its task too, but only until it ends, which is
    /// when it has to say which subagent ended.
    pub(super) started: BTreeMap<TaskId, Started>,
    /// The newest list, to say again once `task_started` says more of one of its tasks that changes what's shown.
    pub(super) newest: Vec<AgentTask>,
    /// The tasks running in the foreground, as every command the agent runs is, from its `task_started` until its
    /// `task_notification`, or until it's moved to the background. Its call and result say everything of it, so the
    /// lines about it are kept for nothing.
    pub(super) foreground: BTreeSet<TaskId>,
}

/// What the call that started a task says of it, which is open as its `task_started` comes.
#[derive(Debug)]
pub(super) struct Started {
    call: ToolUseId,
    /// It's a `Monitor` call's.
    monitor: bool,
    /// The subagent whose call it is, rather than the agent's. The CLI lists its tasks with the agent's own, but tells
    /// the subagent how each ends while it works, and seldom the agent once it has ended.
    by: Option<SubagentId>,
    /// The command it runs, if its caller didn't say what it's for, which the CLI then describes it as.
    command: Option<String>,
}

impl Tasks {
    /// Notes what this line says of a task. The CLI lists a task that starts in the background one line before
    /// `task_started` names the call that started it, so that line is the first to say which call that was, and
    /// whether it's a monitor, a subagent's, or a command nobody described. For each of those, and for a subagent,
    /// whose line shows its button once its call is known, this returns the newest list to say again. Any other
    /// command's call changes nothing shown, so it waits for the next list. As a subagent starts or ends, what it has
    /// in the background becomes part of its work, or is left with the agent, so the list is said again then too.
    /// [`Tasks::listed`] gives a list its calls and kinds. `task_started` comes before its call's result, so the call
    /// is still among `calls`.
    pub(in super::super) fn note(&mut self, value: &Value, calls: &OpenCalls) -> Option<EventBody> {
        match (str_of(value, "type"), str_of(value, "subtype")) {
            (Some("system"), Some("task_started")) => return self.note_started(value, calls),
            (Some("system"), Some("task_notification")) => {
                let id = id_of::<TaskId>(value, "task_id")?;
                let part_of_it = |task: &AgentTask| task.part_of.as_ref().is_some_and(|of| of.as_str() == id.as_str());
                let again = self.newest.iter().any(part_of_it).then(|| self.again());
                if !self.lists(&id) {
                    self.started.remove(&id);
                }
                if str_of(value, "status") == Some("stopped") {
                    self.stopped.insert(id);
                }
                return again;
            }
            _ => {}
        }
        None
    }

    /// The newest list, to say again.
    fn again(&self) -> EventBody {
        EventBody::AgentTasks { tasks: self.newest.clone(), stopped: Vec::new() }
    }

    /// Whether the newest list holds the task `id`.
    fn lists(&self, id: &TaskId) -> bool {
        self.newest.iter().any(|task| task.id == *id)
    }

    /// Whether the newest list holds a task that the subagent running as the task `id` started. It's named by that
    /// task, as `super::super::subagents::Subagents` names it.
    fn has_tasks(&self, id: &TaskId) -> bool {
        let of_it = |started: &Started| started.by.as_ref().is_some_and(|by| by.as_str() == id.as_str());
        self.newest.iter().any(|task| self.started.get(&task.id).is_some_and(of_it))
    }

    pub(super) fn note_started(&mut self, value: &Value, calls: &OpenCalls) -> Option<EventBody> {
        let call: ToolUseId = id_of(value, "tool_use_id")?;
        let id: TaskId = id_of(value, "task_id")?;
        let known = self.started.remove(&id);
        let started = match (calls.tool(&call), known) {
            // Started again under a call that's been answered, which says nothing more.
            (None, Some(known)) => Started { call, ..known },
            (tool, _) => Started {
                monitor: tool == Some(MONITOR),
                by: calls.by(&call).cloned(),
                command: calls.undescribed_command(&call).map(str::to_string),
                call,
            },
        };
        let is_subagent = str_of(value, "task_type") == Some(SUBAGENT);
        let shown = is_subagent || started.monitor || started.by.is_some() || started.command.is_some();
        let again = (shown && self.lists(&id)) || self.has_tasks(&id);
        self.started.insert(id, started);
        again.then(|| self.again())
    }

    /// Whether this line is about a task in the foreground, noting what it says of which are: one whose `task_started`
    /// says `is_backgrounded: false`, until a `task_updated` moves it to the background, which is the first of its lines
    /// that isn't, or its `task_notification`, the last that is, after which it's forgotten. Every line comes here, a
    /// subagent's among them, though what it makes is read from elsewhere, so no task is noted after it ends.
    pub(in super::super) fn in_foreground(&mut self, value: &Value) -> bool {
        if str_of(value, "type") != Some("system") {
            return false;
        }
        let Some(id) = id_of::<TaskId>(value, "task_id") else { return false };
        match str_of(value, "subtype") {
            // A task started again, as a subagent can be, is where its new start says.
            Some("task_started") => {
                let in_foreground = value.get("is_backgrounded").and_then(Value::as_bool) == Some(false);
                if in_foreground {
                    self.foreground.insert(id);
                } else {
                    self.foreground.remove(&id);
                }
                in_foreground
            }
            Some("task_updated") if value.pointer("/patch/is_backgrounded").and_then(Value::as_bool) == Some(true) => {
                self.foreground.remove(&id);
                false
            }
            Some("task_progress" | "task_updated") => self.foreground.contains(&id),
            Some("task_notification") => self.foreground.remove(&id),
            _ => false,
        }
    }

    /// The task `id` as the newest list has it, with what the lines before it said.
    pub(in super::super) fn task(&self, id: &TaskId) -> Option<&AgentTask> {
        self.newest.iter().find(|task| task.id == *id)
    }

    /// Whether a subagent's call started the task `id`, so the agent doesn't hear how it ends, or seldom does.
    pub(in super::super) fn a_subagents(&self, id: &TaskId) -> bool {
        self.started.get(id).is_some_and(|started| started.by.is_some())
    }

    /// Says what the call that started each of a list's tasks says of it, and which are part of the work of one of
    /// `subagents`, and returns those the CLI said it stopped that the list leaves out, which it then forgets.
    pub(in super::super) fn listed(&mut self, tasks: &mut [AgentTask], subagents: &Subagents) -> Vec<TaskId> {
        let left = |id: &TaskId| self.newest.iter().any(|t| t.id == *id) && tasks.iter().all(|t| t.id != *id);
        self.started.retain(|id, _| !left(id));
        for task in tasks.iter_mut() {
            let Some(started) = self.started.get(&task.id) else { continue };
            if started.monitor {
                task.kind = TaskKind::Monitor;
            }
            task.call = Some(started.call.clone());
            task.part_of = started.by.clone().filter(|by| subagents.at_work(by));
            if let Some(command) = &started.command {
                (task.description, task.command) = (None, Some(command.clone()));
            }
        }
        self.newest = tasks.to_vec();
        let (gone, still_listed): (BTreeSet<_>, _) =
            std::mem::take(&mut self.stopped).into_iter().partition(|id| tasks.iter().all(|t| t.id != *id));
        self.stopped = still_listed;
        gone.into_iter().collect()
    }
}

/// What a line about one task makes: nothing for a task in the foreground, as [`Tasks::in_foreground`] says, and for one
/// in the background a [`EventBody::TaskChanged`], so the line is kept with a form of its own, though the list and the
/// turns the agent starts by itself say what it means. `None` for any other line, and for one that names no task.
pub(in super::super) fn changed(value: &Value, in_foreground: bool) -> Option<Parsed> {
    if str_of(value, "type") != Some("system") {
        return None;
    }
    let change = match str_of(value, "subtype")? {
        "task_started" => TaskChange::Started,
        "task_progress" => TaskChange::Progressed,
        "task_updated" => TaskChange::Updated,
        "task_notification" => TaskChange::Ended,
        _ => return None,
    };
    let task = id_of::<TaskId>(value, "task_id")?;
    Some(if in_foreground { Parsed::Drop } else { Parsed::Event(EventBody::TaskChanged { task, change }) })
}

/// How the task a `task_notification` is about ended, as its `status` says.
pub(in super::super) fn outcome(value: &Value) -> TaskOutcome {
    match str_of(value, "status") {
        Some("completed") => TaskOutcome::Finished,
        Some("failed") => TaskOutcome::Failed,
        Some("stopped") => TaskOutcome::Stopped,
        _ => TaskOutcome::Unknown,
    }
}
