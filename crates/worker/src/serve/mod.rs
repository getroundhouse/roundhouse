//! The worker's main loop: what it serves a hub on a connection, and what it keeps between connections.

mod agent;
mod connection;
#[cfg(unix)]
mod dialing;
mod hello;
#[cfg(unix)]
mod listen;
mod moves;
mod outbox;
mod scripts;
mod worktrees;

#[cfg(unix)]
pub use dialing::{DialFailed, serve_dialing};
#[cfg(unix)]
pub use listen::listen;
pub(crate) use outbox::{HubConnection, Outbox, Outgoing};

mod worker;

use hello::hello;
pub use worker::{Config, serve};
use worker::{RunningAgent, State, Worker, handle};
