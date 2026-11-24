//! A wait, which the hub keeps, as an MCP task whose ID is the wait's, so the agent's turn can end while it waits, and
//! its CLI hears how it ended by asking `tasks/get`. The task is only a copy of the hub's wait, so it outlasts the
//! worker, the hub and the agent's process alike. It renders a wait the singular `wait_for_pull_request` tool started
//! too, which the hub keeps as ended, and which a CLI resumed from before can still ask after.

#[cfg(test)]
use protocol::model::{Wait, WaitEnd, WaitUntil};
#[cfg(test)]
use serde_json::{Value, json};

#[cfg(test)]
pub(super) mod tests;

mod waiting;
pub(super) use waiting::{cancel, get};

pub(super) use waiting::{how, iso, task};
