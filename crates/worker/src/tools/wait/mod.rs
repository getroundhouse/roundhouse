//! `wait_for_pull_request`: a wait on the workspace's pull request, which the hub keeps, answered as an MCP task whose
//! ID is the wait's, so the agent's turn can end while it waits, and its CLI hears how it ended by asking `tasks/get`.
//! The task is only a copy of the hub's wait, so it outlasts the worker, the hub and the agent's process alike.

#[cfg(test)]
use protocol::model::{PullRequest, Wait, WaitEnd, WaitUntil};
#[cfg(test)]
use serde_json::{Map, Value, json};

#[cfg(test)]
pub(super) mod tests;

mod waiting;
pub(super) use waiting::{NAME, call, cancel, definition, get};
#[cfg(test)]
use waiting::{POLL_INTERVAL_MS, task, until};
