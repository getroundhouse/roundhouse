//! What goes wrong updating, what it means for the person using the app, and when the sidebar says so.

#[cfg(test)]
use super::answer::Answer;
#[cfg(test)]
use std::time::{Duration, SystemTime};

#[cfg(test)]
mod tests;

mod failure;

pub(crate) use failure::Said;
pub use failure::{Kind, Problem, Refusal, Step};
pub(super) use failure::{OutOfReach, Shown, problem};
