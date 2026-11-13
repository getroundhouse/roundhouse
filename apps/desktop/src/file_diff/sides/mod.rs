//! A changed file's two sides, as the diff draws them: each line's text with its tabs as spaces, in its colours, and
//! the words that changed within the lines a change removed and added. Given the file as it was in the diff's commit,
//! each side is whole, since what git leaves out between its hunks is the same on both. Without it, each side has only
//! the lines of the hunks, and of a file too big to be read whole, the lines read of it a few at a time.

#[cfg(test)]
use crate::patch::FilePatch;
#[cfg(test)]
use protocol::model::PageLine;
#[cfg(test)]
use std::ops::Range;

#[cfg(test)]
mod tests;

mod lines;
#[cfg(test)]
pub(super) use lines::Side;
pub(super) use lines::{Extent, Run, Sides};
