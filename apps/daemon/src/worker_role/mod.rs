//! The daemon as a worker: on its hub's pipes, as one that can outlive its hub, which listens on a socket in its state
//! directory, and as the command a hub runs to reach that one, or as one that dials its hub with a ticket.

#[cfg(unix)]
mod dialing;
mod outliving;

mod transport;
#[cfg(unix)]
pub(crate) use dialing::Dial;
#[cfg(unix)]
use transport::config;
pub(crate) use transport::{Given, Transport, run};
