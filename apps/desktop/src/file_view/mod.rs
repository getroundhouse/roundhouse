//! A file as it is in the worktree, as the workspace's file tab shows it when it's opened from All files: its lines,
//! numbered and highlighted for its language, in a list that scrolls both ways, its image, fitted to the tab, with
//! what kind it is and how big, or its video, as `video` says. A Markdown file shows rendered too, as `markdown` says.
//! Which file is the pane's to say, since the file tab is the pane's.

pub(crate) use lines::{Line, Lines, TAB};
pub(crate) use markdown::is_markdown;
pub(crate) use reading::{HIGHLIGHT_UP_TO, Read, Unshown, hash, what_it_gives};
pub(crate) use video::SystemOpens;
#[cfg(any(test, feature = "testing"))]
pub use video::opened_with_system;

mod lines;
mod markdown;
mod reading;
mod video;

mod viewer;
use viewer::Contents;
pub use viewer::FileView;
pub(crate) use viewer::{LINE_HEIGHT, numbers_width};
