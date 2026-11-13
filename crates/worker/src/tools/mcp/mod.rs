//! MCP's messages for the worker's tools, on protocol `2026-07-28`: what each request asks, and its answer. Only that
//! version, which has no handshake, so each request is answered alone, from what it says. A wait is an MCP task, of
//! the tasks extension, for a client that offers to take one.

#[cfg(test)]
use super::{pull_request, wait};
#[cfg(test)]
use crate::asks::Asks;
#[cfg(test)]
use protocol::Build;

#[cfg(test)]
mod tests;

mod wire;
#[cfg(test)]
pub(super) use wire::{Answer, VERSION};
#[cfg(test)]
use wire::{CAPABILITIES_KEY, TASKS, Tool, VERSION_KEY};
pub(super) use wire::{Called, Code, Headers, Reply, RpcError, Said, answer};
