//! Selecting what a terminal shows with the pointer, as a terminal app does: a press and a drag select from cell to
//! cell, a second press selects the word there and a third the line, and a drag after either goes on by words or by
//! lines. Ghostty's gesture works that out from what each press selects by, and keeps the place the press was as the
//! program writes on. What it selects is the terminal's own selection, which Ghostty keeps on the same text as the
//! screen scrolls.

mod gesture;
#[cfg(test)]
mod tests;

pub(super) use gesture::Selecting;
pub use gesture::{Pointer, SelectBy};
