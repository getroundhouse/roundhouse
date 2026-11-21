//! A terminal given a new size by a client, which names the cells it wants kept through it, such as the top of its
//! view and its selection's ends, and is told where each went once the terminal has rewrapped its rows, since a new
//! size starts a new numbering.

use serde::{Deserialize, Serialize};

use super::{Numbering, TerminalPoint};

/// The most cells a client names to keep: its view's top and its selection's ends, with room to spare. The worker
/// keeps a reference to each while it resizes, which costs a little on every change to the terminal while it's kept.
pub const MOST_KEPT_POINTS: usize = 8;

/// Where each cell a client named went, in the numbering the terminal is in after the new size, in the order they
/// were named. None for a cell in another numbering than the one named, or one the terminal no longer keeps.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TerminalFitReply {
    pub numbering: Numbering,
    pub kept: Vec<Option<TerminalPoint>>,
}
