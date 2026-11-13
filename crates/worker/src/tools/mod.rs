//! The worker's tools, which it serves to its agents over MCP, on 127.0.0.1, and answers by asking its hub. Each agent
//! is given a token by its hub, which says whose a call is, so the server is for this worker's agents alone.

#[cfg(test)]
use crate::asks::Asks;
#[cfg(test)]
use protocol::Build;
#[cfg(test)]
use std::net::SocketAddr;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use tokio::net::TcpSocket;

mod http;
mod mcp;
mod pull_request;
#[cfg(test)]
mod tests;
mod wait;

mod server;
use server::PATH;
pub(crate) use server::Tools;
#[cfg(test)]
use server::{Serving, answered, listen};
