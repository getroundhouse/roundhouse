//! One session's log, kept current so a view only reads it: a window of its history and its live tail. A view
//! follows every event, or only the agent's own, or one subagent's.

#[cfg(test)]
use super::follow::FollowError;
pub use search::{Search, Searched, Stretch};

mod search;
mod source;

#[cfg(test)]
pub(super) mod tests;

mod loading;
pub use loading::{OpenSession, View};
use source::read_range;
