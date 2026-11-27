//! The daemon as a worker: on its hub's pipes, as one that dials its hub with a ticket, so it can outlive its hub, or
//! as one that listens on a socket in its state directory, as an earlier build's did, and the command a hub runs to
//! reach that one.

#[cfg(unix)]
mod dialing;
mod outliving;

mod transport;
#[cfg(unix)]
pub(crate) use dialing::Dial;
#[cfg(unix)]
use transport::config;
pub(crate) use transport::{Given, Transport, run};
