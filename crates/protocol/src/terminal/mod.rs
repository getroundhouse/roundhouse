//! What goes between a client, its hub and a workspace's worker for a terminal: what it shows, as rows, how big it is,
//! and what's pressed in it.
//!
//! The shell runs in its worker, which keeps the only copy of what the terminal shows. A client that watches is sent
//! pictures of its rows, and asks the worker for the rest, such as rows of its history and the text of a selection.

#[cfg(test)]
mod tests;

mod fit;
mod history;
mod palette;
mod picture;
mod rows;
mod sending;
mod size;
mod text;
mod typed;
mod updates;

pub use fit::{MOST_KEPT_POINTS, TerminalFitReply};
pub use history::{MOST_ROWS, TerminalRowsReply};
pub use palette::DEFAULT_PALETTE;
pub use picture::{CursorShape, PaletteColor, RowJson, RowRun, ScreenState, TerminalCursor, TerminalPicture, Wheel};
pub use rows::{CellAttributes, CellColor, CellStyle, Numbering, RowNumber, Span, TerminalRow, Underline};
pub use sending::{MOST_PICTURE_BYTES, PictureSending, Shown};
pub use size::TerminalSize;
pub use text::{Bounds, MOST_TEXT_BYTES, TerminalBoundsReply, TerminalPoint, TerminalRange, TerminalTextReply};
pub use typed::{Key, KeyPress, Mods, Typed};
pub use updates::{TerminalExit, TerminalUpdate};
