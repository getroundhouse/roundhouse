//! Opens the real app on a screen's hub, in a window that's never on screen, and takes its picture.

#![cfg(test)]

mod dictating;
mod files;
mod finding;
mod opening;
mod proposals;
mod scrollbar;

pub(crate) use finding::{FoundBothWays, copied_whole, found_both_ways};
use opening::open;

mod driver;
use driver::{Driver, TIMEOUT, selected_pane};
pub(crate) use driver::{Picture, take};
