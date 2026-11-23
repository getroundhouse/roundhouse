//! How a worker dials its hub over iroh, as `docs/decisions/workers-dial-the-hub.md` says: the endpoint each side
//! binds, and a ticket's [`HubAddress`] as iroh reads it. The hub and the worker both use it, so how their endpoints
//! are set up, and which iroh they're on, have one home. What they say on a connection is in `protocol::dialing`.
//!
//! Neither side publishes its addresses to n0's address lookup: a ticket names the hub's relay and its direct
//! addresses, and a worker reaches the hub through those alone. The hub opens a direct path to a worker at the
//! addresses the worker advertises, such as the host's end of a port forwarded into a VM.

#![warn(clippy::wildcard_enum_match_arm)]

mod admission;
mod error;
mod hub_endpoint;
mod key_file;
mod worker_endpoint;

#[cfg(test)]
mod tests;

pub use admission::{read_frame, write_frame};
pub use error::DialError;
pub use hub_endpoint::{HubEndpoint, Relays};
pub use iroh::endpoint::{Connection, Incoming, RecvStream, SendStream};
pub use worker_endpoint::WorkerEndpoint;

/// The key a connection's other end proved it holds, as iroh writes it, which is how the hub knows a worker again.
pub fn remote_key(connection: &Connection) -> String {
    connection.remote_id().to_string()
}
