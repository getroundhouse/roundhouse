//! A hub owns state and serves the platform API. It doesn't do the work itself: workers do, and
//! providers say where they run.

#![warn(clippy::wildcard_enum_match_arm)]

mod attachment_files;
mod calls;
mod child_process;
mod error;
mod frames;
mod hub;
mod listener;
mod names;
mod private;
mod provider_link;
mod provider_set;
mod run;
mod server;
mod session;
pub mod store;
mod terminate;
mod versions;
mod worker_link;

pub use crate::error::HubError;
pub use crate::run::{HubConfig, run};

/// The names the hub picks a new workspace's from, so a test can have git hold every one.
#[cfg(feature = "testing")]
#[must_use]
pub fn workspace_names() -> &'static [&'static str] {
    names::NAMES
}
