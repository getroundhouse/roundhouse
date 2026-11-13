//! Starting a hub, and running it until it's asked to stop.

#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
mod tests;

mod startup;
pub use startup::{HubConfig, run};
#[cfg(test)]
use startup::{HubLock, recover};
