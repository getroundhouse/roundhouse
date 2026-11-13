//! Where a transcript is scrolled to, and what's known of its rows' heights, between one frame and the next.

#[cfg(test)]
use super::layout::{Anchor, Start};
#[cfg(test)]
use std::ops::Range;

mod heading;
mod scrollbar;
mod still;

#[cfg(test)]
mod tests;

mod keeping;
use keeping::Keeping;
pub use keeping::Place;
pub(super) use keeping::State;
