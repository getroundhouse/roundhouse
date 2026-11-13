//! What goes between a client, its hub and a workspace's worker for a terminal: what the program in it writes, how big
//! it is, and what it shows, for a client that attaches partway through.
//!
//! The shell runs in its worker, which keeps what the terminal shows. A client that attaches gets that as a screen,
//! then everything after it, in order, so its own terminal shows the same as the worker's, as "Terminals" in
//! `docs/architecture.md` says.

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

crate::base64_bytes::base64_bytes!(
    /// Bytes a terminal's program wrote, or that were typed into it. They needn't be text, and a character or a
    /// sequence can be split between one and the next.
    TerminalBytes
);

/// Where an update is among a terminal's: how many of them its worker sent before it, from when its shell started.
/// The first is 0, and there are no gaps, so a hub can tell which come after a screen. Every update counts, not only
/// what the program wrote, so a screen that took in a new size is never followed by it again.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct UpdateIndex(u64);

impl UpdateIndex {
    pub const fn new(index: u64) -> Self {
        Self(index)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// The one after this.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// What a terminal shows: the bytes that make a fresh terminal of `size` show the same, with its history, its colours,
/// the cursor and the modes the program set, so what the program writes next lands there as it does in the worker's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TerminalScreen {
    pub size: TerminalSize,
    pub bytes: TerminalBytes,
}

/// How a terminal's shell ended.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalExit {
    /// What it exited with. `None` when a signal ended it, or when how it ended isn't known, as when its worker went.
    pub code: Option<i32>,
}

/// What happens next in a terminal, in the order it happened.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "update", rename_all = "snake_case")]
pub enum TerminalUpdate {
    /// Replaces what the terminal shows: a client gives its terminal the screen's size, empties it, and writes the
    /// screen's bytes. It comes first to a client that attaches, and again whenever one fell too far behind to be sent
    /// every byte, so nothing queues for it without limit.
    Screen(TerminalScreen),
    /// What the program wrote.
    Output { output: TerminalBytes },
    /// The terminal has a new size from here on, which a client gives its own before what comes next, so every client
    /// shows what the program draws for it, whoever resized it.
    Resized { size: TerminalSize },
    /// The shell ended, and nothing comes after.
    Exited(TerminalExit),
    /// Any update this build doesn't know, from a peer newer than it. A client skips it.
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests;
