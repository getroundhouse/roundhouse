//! One changed file's diff, as the workspace's file tab shows it, with the comments on it and the one being written, or
//! a file that isn't text, as an [`ImageDiff`](crate::image_diff::ImageDiff). Which file is the pane's to say, since the file tab is the pane's, and
//! the files and comments are the workspace's [`Review`](crate::review::Review)'s.
//!
//! The app draws the diff itself, rather than with the kit's `Diff`, which shows only the lines in git's patch, and
//! keeps what's open, where it's scrolled to and what's selected with what it draws, so a diff loaded again lost them.
//! Here what's open of each file, and where the list is, are this view's, and the lines git left out are read from
//! the file as it was. `docs/gpui.md` has why.

pub use draft::{Draft, LineAt};
pub(crate) use selection::KEY_CONTEXT;
pub use view::DiffView;

mod annotations;
mod draft;
mod draw;
mod layout;
mod list;
mod opened;
mod selection;
mod shown;
mod sides;
mod view;

mod diff_view;
pub use diff_view::{FileDiff, FileDiffEvent};
