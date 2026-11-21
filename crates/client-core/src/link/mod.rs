//! The connection to one hub, made again whenever it goes, with the hub's records kept on every connection, so an app
//! only draws how the link stands. Every client reconnects by the same rule: a failure that trying again can't fix,
//! such as a revoked token, waits for a person to ask again; any other is tried again after a wait that doubles; a
//! connection that goes is made again at once; and a new connection that doesn't catch up on the records is dropped.
//! Each app gives the link the way it connects, since only the desktop app starts a hub, and draws what it says.

mod connect;
mod handle;
mod linker;
mod state;
#[cfg(test)]
mod tests;
mod waits;

pub use connect::{Connect, Lost, Refused};
pub use handle::Link;
pub use state::LinkState;
pub use waits::Waits;
