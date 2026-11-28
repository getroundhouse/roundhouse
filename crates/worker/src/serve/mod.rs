//! The worker's main loop: what it serves a hub on a connection, and what it keeps between connections.

mod agent;
mod connection;
mod dialing;
mod hello;
mod moves;
mod outbox;
mod scripts;
mod worktrees;

pub use dialing::{DialFailed, serve_dialing};
pub(crate) use outbox::{HubConnection, Outbox, Outgoing};

mod worker;

use hello::hello;
pub use worker::{Config, serve};
use worker::{RunningAgent, State, Worker, handle};
