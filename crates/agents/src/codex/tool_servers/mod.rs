//! What Codex says of the agent's tool servers, which the adapter reports whole as it changes.

mod tracking;
mod why;

pub(super) use tracking::{STARTUP_MOVED, ToolServers, moved, reload, turn_started};
