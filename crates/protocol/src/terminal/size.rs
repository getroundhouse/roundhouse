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
    /// The most a worker gives a terminal, which a client asks for when its view has room for more. Few enough cells,
    /// 120,000, that a whole picture of one stays small on a slow link, where 2,000 by 1,000, the most before, was 2
    /// million.
    pub const MOST: Self = Self { cols: 600, rows: 200 };

    /// Whether a worker gives a terminal this size.
    pub fn is_allowed(self) -> bool {
        (1..=Self::MOST.cols).contains(&self.cols) && (1..=Self::MOST.rows).contains(&self.rows)
    }
}
