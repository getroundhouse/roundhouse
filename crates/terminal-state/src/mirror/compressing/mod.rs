//! Compressing a copy's history once its terminal has been quiet a while, a few of Ghostty's steps at a time, as Ghostty
//! means it to be done.

mod schedule;
#[cfg(test)]
mod tests;

pub(super) use schedule::Compressing;
pub use schedule::QUIET_BEFORE_COMPRESSING;
