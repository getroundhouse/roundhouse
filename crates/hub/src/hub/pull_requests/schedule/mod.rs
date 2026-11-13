//! Which workspace's pull request to read next when nobody has asked: soonest what's likely to have changed, and
//! less and less often what keeps coming back the same.

#[cfg(test)]
use super::Asked;

#[cfg(test)]
mod tests;

mod scheduling;
#[cfg(test)]
use scheduling::{LATEST, SOONEST};
pub(super) use scheduling::{Schedule, Went, due};
