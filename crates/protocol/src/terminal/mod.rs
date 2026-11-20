//! What goes between a client, its hub and a workspace's worker for a terminal: what the program in it writes, how big
//! it is, and what it shows, for a client that attaches partway through.
//!
//! The shell runs in its worker, which keeps what the terminal shows. A client that attaches gets that as a screen,
//! then everything after it, in order, so its own terminal shows the same as the worker's.

#[cfg(test)]
mod tests;

mod held;
mod size;
mod updates;

pub use held::{HeldUpdates, JOINED_OUTPUT_BYTES};
pub use size::TerminalSize;
pub use updates::{TerminalBytes, TerminalExit, TerminalScreen, TerminalUpdate, UpdateIndex};
