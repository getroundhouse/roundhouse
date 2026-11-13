//! The tools over HTTP: MCP's Streamable HTTP, as protocol `2026-07-28` has it, with each request answered as JSON.
//! Only from this machine, and only with a token the hub gave one of this worker's agents.

#[cfg(test)]
use super::Tools;
#[cfg(test)]
use super::mcp::Said;

#[cfg(test)]
mod tests;

mod connection;
pub(super) use connection::serve_connection;
#[cfg(test)]
use connection::{MAX_BODY_BYTES, METHOD_HEADER, VERSION_HEADER, decoded, respond};
