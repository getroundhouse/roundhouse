//! A file as it is in the worktree, as the workspace's file tab shows it when it's opened from All files: its lines,
//! numbered and highlighted for its language, in a list that scrolls both ways, or its image, fitted to the tab, with
//! what kind it is and how big. Which file is the pane's to say, since the file tab is the pane's.

pub(crate) use lines::{Line, Lines, TAB};
pub(crate) use reading::{HIGHLIGHT_UP_TO, Read, Unshown, hash, what_it_gives};

mod lines;
mod reading;

mod viewer;
use viewer::Contents;
pub use viewer::FileView;
pub(crate) use viewer::{HEADER_HEIGHT, LINE_HEIGHT, numbers_width, placeholder};
