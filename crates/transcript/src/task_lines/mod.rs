//! Which lines the end of the chat has for the tasks the agent runs in the background. Each task has a line of its
//! own while there's room for them. Past that, each group with several has one line, which opens to its tasks' own
//! lines, so closed, the lines are never more than there are groups, however many tasks there are.

#[cfg(test)]
use crate::folds::{Fold, Folds};
#[cfg(test)]
use protocol::event::{AgentTask, TaskKind};

#[cfg(test)]
mod tests;

mod groups;
#[cfg(test)]
pub(crate) use groups::ROOM;
pub(crate) use groups::{TaskGroup, TaskLine, forget_gone, task_lines};
