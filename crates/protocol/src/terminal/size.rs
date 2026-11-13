//! How big a terminal is, and the most a worker gives one.

use serde::{Deserialize, Serialize};

/// How many columns and rows a terminal has. A worker refuses one with no columns or no rows, or with more than
/// [`TerminalSize::MOST`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

impl TerminalSize {
    /// The most a worker gives a terminal. Far more than a screen shows at any size of type someone can read, and few
    /// enough cells that the copies of a terminal that keep every one, the worker's and each client's, fit in memory
    /// easily, as a size a bug made up might not.
    pub const MOST: Self = Self { cols: 2000, rows: 1000 };

    /// Whether a worker gives a terminal this size.
    pub fn is_allowed(self) -> bool {
        (1..=Self::MOST.cols).contains(&self.cols) && (1..=Self::MOST.rows).contains(&self.rows)
    }
}
