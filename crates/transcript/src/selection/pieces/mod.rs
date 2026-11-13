//! A plain text as the transcript lays it out: in pieces, each a text of its own, such as a line of what a command
//! printed, in order and a line break apart, which together say all of it. They turn a point into a place in the text,
//! for a press, and paint what's selected of it: behind the pieces, to the edge of a row that goes on into the next,
//! and over the space between two pieces a selection runs across, so it reads as one. The kit does both for a leaf of
//! Markdown, and makes neither for a text it didn't lay out, so this follows what it does.

#[cfg(test)]
mod tests;

mod geometry;
pub(crate) use geometry::Pieces;
#[cfg(test)]
use geometry::{Span, boxes, highlight};
