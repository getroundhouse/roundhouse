//! The tools over HTTP: MCP's Streamable HTTP, with each request answered as JSON, and beside them each of the hub's
//! tool servers, whose answers the proxy passes on. Only from this machine, and only with a token the hub gave one of
//! this worker's agents.

#[cfg(test)]
use super::Tools;
#[cfg(test)]
use super::mcp::Said;

#[cfg(test)]
mod tests;

mod answer;
mod connection;
use answer::empty;
pub(super) use answer::{AnswerBody, whole};
pub(super) use connection::serve_connection;
#[cfg(test)]
use connection::{METHOD_HEADER, Route, VERSION_HEADER, decoded, respond};
