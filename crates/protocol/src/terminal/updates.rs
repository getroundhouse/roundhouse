//! How a terminal's shell ended, which its worker sends in order with the rest of what it sends of it.

use serde::{Deserialize, Serialize};

/// How a terminal's shell ended.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalExit {
    /// What it exited with. `None` when a signal ended it, or when how it ended isn't known, as when its worker went.
    pub code: Option<i32>,
}

/// What happens next in a terminal, besides what it shows, which pictures say.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "update", rename_all = "snake_case")]
pub enum TerminalUpdate {
    /// The shell ended, and nothing comes after.
    Exited(TerminalExit),
    /// Any update this build doesn't know, from a peer newer than it, or from a worker from before rows, which sent what
    /// its program wrote. A client skips it.
    #[serde(other)]
    Unknown,
}
