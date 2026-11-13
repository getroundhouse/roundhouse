//! Removing the copies of the daemon that nothing runs from anymore. Every build that starts a hub here leaves one,
//! and an update can come several times a day.

#[cfg(test)]
use super::temporary_prefix;
#[cfg(test)]
use protocol::paths::Paths;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::time::{Duration, SystemTime};

#[cfg(test)]
mod tests;

mod cleanup;

#[cfg(test)]
use cleanup::ABANDONED_AFTER;
pub(super) use cleanup::remove_unused;
