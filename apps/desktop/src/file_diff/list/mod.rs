//! The list a file's diff is drawn in, which draws only the rows in sight, however long the file is. A row is a line
//! or a fold tall, apart from a comment's, which is measured as it's drawn and kept by which comment it is, so it keeps
//! its height when the rows are laid out again. Its place is a row and how far into it, so a row above that changes
//! height moves nothing in sight. The code in it scrolls sideways under the line numbers, which stay where they are, as
//! the comments do.
//!
//! It's the diff's own, rather than GPUI's list, which forgets every height it has when its width changes, and counts
//! a row it hasn't drawn as nothing. The kit's scrollbar, and its bounce at the ends, which hears the wheel up and
//! down, work on anything that says how far it's scrolled and how tall it is, which this does for them.

mod element;

mod diff_list;

use diff_list::State;
pub(super) use diff_list::{Code, DiffList, Height, Place};
