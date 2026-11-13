//! Which lines the end of the chat has for the tasks the agent runs in the background. Each task has a line of its
//! own while there's room for them. Past that, each group with several has one line, which opens to its tasks' own
//! lines, so closed, the lines are never more than there are groups, however many tasks there are.

use protocol::event::{AgentTask, TaskKind};

use crate::folds::{Fold, Folds};
use crate::item_key::ItemKey;

/// How many tasks get a line each. With more, a group with several is gathered into one line, so there are at most as
/// many lines as groups. Four is as many as read at a glance under the working line.
pub(crate) const ROOM: usize = 4;

/// The background tasks of one kind, as one line can name several of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TaskGroup {
    Subagents,
    Commands,
    Monitors,
}

impl TaskGroup {
    /// The order their lines come in: the agent's own work, then what runs, then what only watches.
    const ALL: [TaskGroup; 3] = [TaskGroup::Subagents, TaskGroup::Commands, TaskGroup::Monitors];

    /// The group a task's line is in. `None` for a wait on the pull request, which has no line here: the hub keeps
    /// the wait, and its line is drawn from the hub's.
    fn of(task: &AgentTask) -> Option<Self> {
        match task.kind {
            TaskKind::Subagent => Some(TaskGroup::Subagents),
            TaskKind::Command => Some(TaskGroup::Commands),
            TaskKind::Monitor => Some(TaskGroup::Monitors),
            TaskKind::Wait => None,
        }
    }

    /// What several of them are called, as their line says after how many there are, and as its element is named.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TaskGroup::Subagents => "subagents",
            TaskGroup::Commands => "commands",
            TaskGroup::Monitors => "monitors",
        }
    }

    /// What the line of one says when the agent didn't say what it's for.
    pub(crate) fn one(self) -> &'static str {
        match self {
            TaskGroup::Subagents => "A subagent",
            TaskGroup::Commands => "A command",
            TaskGroup::Monitors => "A monitor",
        }
    }

    /// What the line for several of them opens and closes by.
    pub(crate) fn key(self) -> ItemKey {
        ItemKey::Tasks(self)
    }
}

/// A line at the end of the chat for what's in the background.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TaskLine<'a> {
    /// A task with a line of its own.
    Task(TaskGroup, &'a AgentTask),
    /// Several of a group in one line, in the order the agent started them, and whether it's open to them.
    Several { group: TaskGroup, tasks: Vec<&'a AgentTask>, open: bool },
    /// One of an open group's tasks, under the group's line.
    Under(TaskGroup, &'a AgentTask),
}

/// The lines for `tasks`, which are in the order the agent started them. Up to [`ROOM`] of them have a line each, in
/// that order. More are listed by group, and a group with several is one line, with its tasks' own lines under it
/// while `folds` has it open.
pub(crate) fn task_lines<'a>(tasks: &'a [AgentTask], folds: &Folds) -> Vec<TaskLine<'a>> {
    let listed = with_lines(tasks);
    if listed.len() <= ROOM {
        return listed.into_iter().map(|(group, task)| TaskLine::Task(group, task)).collect();
    }
    let mut lines = Vec::new();
    for group in TaskGroup::ALL {
        let of_it = of_group(&listed, group);
        match of_it.as_slice() {
            [] => {}
            [only] => lines.push(TaskLine::Task(group, only)),
            _ => {
                let open = folds.is_open(&group.key(), Fold::Tasks);
                let under: Vec<_> = of_it.iter().filter(|_| open).map(|task| TaskLine::Under(group, task)).collect();
                lines.push(TaskLine::Several { group, tasks: of_it, open });
                lines.extend(under);
            }
        }
    }
    lines
}

/// Forgets that a group's line was opened once `tasks` no longer have one for it, so a line that comes back, such as
/// for the next lot of subagents, starts closed.
pub(crate) fn forget_gone(tasks: &[AgentTask], folds: &mut Folds) {
    let listed = with_lines(tasks);
    for group in TaskGroup::ALL {
        if listed.len() <= ROOM || of_group(&listed, group).len() < 2 {
            folds.forget(&group.key(), Fold::Tasks);
        }
    }
}

/// The tasks that have lines, each with its group, in the order the agent started them.
fn with_lines(tasks: &[AgentTask]) -> Vec<(TaskGroup, &AgentTask)> {
    tasks.iter().filter_map(|task| Some((TaskGroup::of(task)?, task))).collect()
}

fn of_group<'a>(listed: &[(TaskGroup, &'a AgentTask)], group: TaskGroup) -> Vec<&'a AgentTask> {
    listed.iter().filter(|(of, _)| *of == group).map(|(_, task)| *task).collect()
}

#[cfg(test)]
mod tests;
