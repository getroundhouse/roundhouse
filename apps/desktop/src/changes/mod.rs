//! The sidebar on the right's Changes: every file the workspace changes against its base, as a list or in folders.
//! Clicking a file shows its diff in the workspace's file tab.

use rows::rows;

mod comments_mark;
mod rows;

mod panel;

pub(crate) use panel::controls;
pub use panel::{Changes, ChangesEvent};
