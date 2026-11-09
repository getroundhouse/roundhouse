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
pub use crate::remote::MAX_UNAUTHENTICATED;
pub use crate::run::{HubConfig, run};
