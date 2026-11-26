//! How the hub and the worker send a request to an MCP server over HTTP, and read what it answers: a worker as it
//! passes on an agent's call, and the hub as it checks a server and signs in to it. One way for both, so a check sends
//! what an agent's call would, follows a redirect only where an agent's call would be, and neither sends a server's key
//! anywhere the other wouldn't, or takes a refusal for something the other doesn't. The same goes for which addresses
//! are private, where a sign-in may go over plain `http`.
//!
//! No client depends on it, as `docs/principles/layers.md` says, since only the hub and the worker reach a server.

#![warn(clippy::wildcard_enum_match_arm)]

#[cfg(test)]
mod tests;

mod address;
pub use address::is_private;
mod challenge;
pub use challenge::{Challenge, access_wanted};
mod connecting;
pub use connecting::{CONNECT_WITHIN, Connector};
mod names;
pub use names::Names;
mod redirect;
mod sending;
pub use sending::send;
mod server;
pub use server::{Redirected, Server};
mod tls;
pub use tls::Tls;
#[cfg(any(test, feature = "testing"))]
mod stand_in;
#[cfg(any(test, feature = "testing"))]
pub use stand_in::{Heard, KEY, StandIn};
