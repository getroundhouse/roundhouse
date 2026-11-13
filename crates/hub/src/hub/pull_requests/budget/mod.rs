//! How many of the calls that count against GitHub's hourly limit the hub makes to read pull requests: a steady
//! number a minute for reads nobody is waiting on, with room for a burst, and never more than the limit can spare. The
//! limit is shared with every agent's own `gh` and whatever else the user runs, so a read nobody is waiting on, or one
//! an agent asks for, leaves part of it alone.

#[cfg(test)]
use super::Asked;

#[cfg(test)]
mod tests;

mod limits;
#[cfg(test)]
use limits::{BURST, GITHUB_SAID_FOR, PER_MINUTE};
pub(super) use limits::{Budget, calls_to_read};
