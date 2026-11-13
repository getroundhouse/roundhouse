//! The rows a file's diff is drawn as: its hunks' lines, in one column or side by side, a fold for each run of lines
//! git left out that isn't open, and each comment below the line it's on. A line with a comment always shows, so a
//! comment is never folded away, and one whose line isn't there now goes at the end.

#[cfg(test)]
use super::opened::Opened;
#[cfg(test)]
use crate::patch::FilePatch;
#[cfg(test)]
use protocol::model::{CommentId, DiffSide};

#[cfg(test)]
mod tests;

mod shape;
pub(super) use shape::{Fold, Note, Old, Row, STEP, Shape, rows, same_row};
