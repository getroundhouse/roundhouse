//! The worker's main loop: what it serves a hub on a connection, and what it keeps between connections.

mod agent;
mod connection;
#[cfg(unix)]
mod listen;
mod moves;
mod outbox;
mod scripts;
mod worktrees;

#[cfg(unix)]
pub use listen::listen;
pub(crate) use outbox::Outbox;

mod worker;

#[cfg(feature = "testing")]
pub use worker::TEST_BUILD;
pub use worker::{Config, serve};
use worker::{RunningAgent, State, Worker, handle, hello};
