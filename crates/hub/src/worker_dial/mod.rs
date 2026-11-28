//! The hub's end of the connections workers dial, as `protocol::dialing` says: two iroh endpoints, one on loopback for
//! the workers on the hub's own machine and one on the hub's relays for the rest, which each admit a worker that holds
//! an unused ticket, or one either admitted before, and hold each admitted worker's connection for whoever waits for
//! its instance's worker.

#[cfg(test)]
mod tests;

mod admitting;
mod arrivals;
mod dial;

pub(crate) use arrivals::Dialed;
pub use dial::EndpointPorts;
pub(crate) use dial::{Endpoint, WorkerDial};
