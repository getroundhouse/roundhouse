//! Which rows are in the window, and where, worked out from one row: one that stays where it was in the last frame,
//! or one sent to the window's top or bottom. A row's place comes from the rows laid out around it, so the rows
//! nobody is looking at are never measured for it. A row is drawn to show it, or to find a height that isn't known.
//! One out of the window whose height is known is only given its place.

#[cfg(test)]
use super::state::Place;
#[cfg(test)]
use gpui_kit::{Pixels, px};

#[cfg(test)]
mod tests;

mod placement;
pub(super) use placement::{Anchor, Layout, Rows, Start, lay_out};
#[cfg(test)]
pub(super) use placement::{LaidRow, OVERDRAW};
