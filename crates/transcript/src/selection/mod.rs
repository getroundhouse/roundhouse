//! What's selected of the chat's texts that `crate::texts` draws, which the transcript holds rather than the window,
//! so it outlasts the rows it's in being drawn. The kit's selection is the window's, over the texts painted in a frame,
//! and lets go of what's selected once a text holding either end of it isn't painted, which in a list that draws only
//! what's in sight is as soon as that text's row leaves the window.
//!
//! Each end is a place in a text: which text, which of its leaves, and a byte in that leaf. The text is named by what
//! its element is known by, which stays the same as it moves, as a reply does from being written to its record, so an
//! end stays in it, and `crate::texts` says where it is now, which orders the ends. Each frame, every text that's drawn
//! says what it's known by and where its leaves are, so a point in the window can be turned into a place in it. A
//! press on a text starts a selection, a drag moves its far end, and with the pointer past the top or bottom of the
//! list, the list scrolls on, faster the further past it is, for as long as the button is held. A copy reads the texts
//! between the ends as they were parsed, or what a plain text said when it was drawn, drawn now or not, which is each
//! the chat has loaded, and each it loaded as the selection was made, since the texts from one end to the other are
//! kept, as is the one a press was on, for a shift-click to extend from. `crate::texts` draws them.
//!
//! It's the mouse's: a press, a drag and a release. The kit's selection also takes a touch screen's long press and
//! handles, which a transcript on a phone will need here.

mod geometry;
mod pieces;
mod places;
mod pointer;
#[cfg(test)]
mod test_layouts;
#[cfg(test)]
mod tests;

pub(crate) use geometry::Geometry;
pub(crate) use pieces::Pieces;

mod selecting;
#[cfg(test)]
use selecting::End;
pub use selecting::KEY_CONTEXT;
pub(crate) use selecting::Selection;
use selecting::{Drawn, Spot};
