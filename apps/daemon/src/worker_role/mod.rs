//! The daemon as a worker: on its hub's pipes, or as one that dials its hub with a ticket, so it can outlive its hub,
//! and as the command a hub runs to reach a worker an earlier build left listening on a socket.

mod attaching;
#[cfg(unix)]
mod dialing;

mod transport;
#[cfg(unix)]
pub(crate) use dialing::Dial;
#[cfg(unix)]
use transport::config;
pub(crate) use transport::{Given, Transport, run};
