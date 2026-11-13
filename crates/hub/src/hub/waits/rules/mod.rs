//! When a wait ends: written once, from the wait and the pull request as a read finds it, the same as it starts and
//! after.

#[cfg(test)]
use crate::store::Seen;
#[cfg(test)]
use protocol::model::{MergeState, PrState, PullRequest, QueueState, WaitEnd, WaitUntil};

#[cfg(test)]
mod tests;

mod settlement;
pub(super) use settlement::{ended, not_yet, seen_in};
