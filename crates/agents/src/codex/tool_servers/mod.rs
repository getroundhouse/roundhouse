//! What Codex says of the agent's tool servers, which the adapter reports whole as it changes.

mod tracking;
mod why;

pub(super) use tracking::{TOOLS_SERVER, ToolServers, moved, turn_started};
