//! Opens the real app on a screen's hub, in a window that's never on screen, and takes its picture.

#![cfg(test)]

mod dictating;
mod files;
mod finding;
mod opening;
mod proposals;
mod recording;
mod scrollbar;
mod voice;

pub(crate) use finding::{FoundBothWays, copied_whole, found_both_ways};
pub(crate) use opening::THIS_MAC;
use opening::open;
pub(crate) use recording::{Done, Pointer, record};

mod driver;
pub(crate) use driver::take;
use driver::{Driver, TIMEOUT, id_in, selected_pane};
pub(crate) use spec_screens::Picture;
