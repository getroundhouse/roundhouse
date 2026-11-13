//! A workspace's pull request as the app shows it: a bar at the top of the sidebar of changes, in the pull request's
//! colour, saying the first thing in the way of merging it and offering the one thing to do about it. While the
//! sidebar is hidden, the header shows its number and that action instead.

#[cfg(test)]
mod tests;
mod words;

pub use words::{Action, Ask};

mod status_bar;
pub(crate) use status_bar::recorded_work;
pub use status_bar::{Lookup, READ_AGAIN, bar, controls};
