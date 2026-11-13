//! Opens the real app on a screen's hub, in a window that's never on screen, and takes its picture.

#![cfg(test)]

mod files;
mod finding;
mod opening;
mod scrollbar;

pub(crate) use finding::{FoundBothWays, copied_whole, found_both_ways};
use opening::open;

mod driver;
use driver::{BETWEEN_PICTURES, Driver, TIMEOUT, selected_pane};
pub(crate) use driver::{Picture, take};
