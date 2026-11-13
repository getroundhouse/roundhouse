//! Telling the CLI where the worker's tools are, in a file given to `--mcp-config`, which keeps the token off its
//! command line, where any process on the machine can read it.

#[cfg(test)]
use crate::ToolServer;
#[cfg(test)]
use std::fs::File;

#[cfg(test)]
mod tests;

mod configuration;
pub(super) use configuration::{TAKES_TASKS, args, forget};
