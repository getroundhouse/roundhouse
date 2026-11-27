//! The daemon as a worker: on its hub's pipes, or as one that dials its hub with a ticket, so it can outlive its hub,
//! and as the command a hub runs to reach a worker an earlier build left listening on a socket.

mod attaching;
mod dialing;

mod transport;
pub(crate) use dialing::Dial;
use transport::config;
pub(crate) use transport::{Given, Transport, run};
