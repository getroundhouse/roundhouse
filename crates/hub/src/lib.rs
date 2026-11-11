//! A hub owns state and serves the platform API. It doesn't do the work itself: workers do, and
//! providers say where they run.

#![warn(clippy::wildcard_enum_match_arm)]

mod attachment_files;
mod error;
mod hub;
mod listener;
mod names;
mod private;
mod run;
mod server;
mod session;
pub mod store;
mod terminate;
mod worker_link;
mod worker_process;

pub use crate::error::HubError;
pub use crate::run::{HubConfig, run};
