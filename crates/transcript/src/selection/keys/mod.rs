//! What the keys do to what's selected in a transcript. With Shift, an arrow moves the end of what's selected that
//! moved last, by a drag, a Shift-click or a key, and the other end stays where it is, as in any text, so a selection
//! made with the mouse can be made a little longer or shorter. Left and Right move it a character, or with the
//! platform's key for a word, to the start of the word before or the end of the word after, through the texts the chat
//! holds. Up and Down move it to the line above or below, as `lines` says.
//!
//! Wherever it goes, it's brought into view, as `sight` says, as a drag keeps the end in view by scrolling the chat on
//! past the window's edge, and a run of them goes on past the edge the same way.

mod lines;
mod moving;
mod sight;
#[cfg(test)]
mod tests;

pub(in crate::selection) use moving::Keyed;
pub use moving::Way;
#[cfg(test)]
use moving::word_beside;
use moving::{Step, Tried, character_beside};
