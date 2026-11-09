//! A hub owns state and serves the platform API. It doesn't do the work itself: workers do, and
//! providers say where they run.

#![warn(clippy::wildcard_enum_match_arm)]

mod error;
mod hub;
mod names;
mod remote;
mod run;
mod server;
mod session;
pub mod store;
mod worker_link;

pub use crate::error::HubError;
pub use crate::run::{HubConfig, run};

/// How many connections from the network can be waiting to authenticate at once, so a test can fill them.
#[cfg(feature = "testing")]
pub const MAX_UNAUTHENTICATED: usize = remote::MAX_UNAUTHENTICATED;

/// How many milliseconds a pairing code works for, in place of ten minutes, so a test can watch one expire.
#[cfg(feature = "testing")]
pub const ENV_PAIRING_LIFE_MS: &str = "ROUNDHOUSE_PAIRING_LIFE_MS";
