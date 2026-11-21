//! Rows of a terminal's history, as its worker sends them when they're asked for by number.

use serde::{Deserialize, Serialize};

use super::rows::{Numbering, RowNumber, TerminalRow};

/// The most rows a worker sends for one ask, so one asks for little of the terminal's thread, whose reading is a few
/// milliseconds at the widest.
pub const MOST_ROWS: u32 = 200;

/// The rows asked for that the terminal has, and where its rows are now, in its numbering now, so a client that asked
/// in one that has gone learns the new.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalRowsReply {
    pub numbering: Numbering,
    /// The oldest row of history the terminal keeps.
    pub first: RowNumber,
    /// The screen's first row, which comes after the last row of history.
    pub top: RowNumber,
    /// The number of the first of `rows`, which is `first` if that's after what was asked for.
    pub from: RowNumber,
    pub rows: Vec<TerminalRow>,
}
