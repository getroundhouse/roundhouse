//! MCP messages for the worker's tools. The 2026 protocol has stateless discovery and tasks; the 2025 protocol
//! initializes and offers ordinary tools. Both use the same authenticated HTTP endpoint and hub asks.

#[cfg(test)]
use super::{pull_request, wait};
#[cfg(test)]
use crate::asks::Asks;
#[cfg(test)]
use protocol::Build;

#[cfg(test)]
mod tests;

mod legacy;
mod wire;
#[cfg(test)]
pub(super) use wire::{Answer, VERSION};
#[cfg(test)]
use wire::{CAPABILITIES_KEY, TASKS, Tool, VERSION_KEY};
pub(super) use wire::{Called, Code, Headers, Reply, RpcError, Said, answer};
