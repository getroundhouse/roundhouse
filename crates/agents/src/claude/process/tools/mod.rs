//! Telling the CLI where its worker's tool servers are, in a file given to `--mcp-config`, which keeps the token off its
//! command line, where any process on the machine can read it.

#[cfg(test)]
use crate::{HubToolServer, ToolServers};
#[cfg(test)]
use std::fs::File;

#[cfg(test)]
mod tests;

mod configuration;
pub(super) use configuration::{TAKES_TASKS, args, forget};
