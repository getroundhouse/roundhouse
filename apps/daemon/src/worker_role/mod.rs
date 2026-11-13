//! The daemon as a worker: on its hub's pipes, or as one that can outlive its hub, which listens on a socket in its
//! instance directory, and as the command a hub runs to reach that one.

mod outliving;

mod transport;
#[cfg(unix)]
use transport::config;
pub(crate) use transport::{Given, Transport, run};
