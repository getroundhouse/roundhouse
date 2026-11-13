//! When to start and stop what keeps the machine awake. It goes by the time it's given rather than the clock, so its
//! rules are tested with made-up times, without waiting for any of them.

#[cfg(test)]
use super::Hold;
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use tokio::time::Instant;

#[cfg(test)]
mod tests;

mod deadlines;
pub(super) use deadlines::{Delays, Step, Timing};
