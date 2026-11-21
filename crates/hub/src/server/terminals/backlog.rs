//! What a terminal's follower holds for a client while the client's last frame of it is still being written, and how
//! far behind the client can fall before it's sent what the terminal shows instead.

use std::collections::VecDeque;

use protocol::terminal::{TerminalBytes, TerminalUpdate};

/// The most output one frame carries when what's held is joined, about what a worker reads from a terminal at a time,
/// so whatever's sent behind it on a slow link waits for little.
pub(super) const JOINED_OUTPUT_BYTES: usize = 64 * 1024;

/// The most output a client can be behind by before it's sent what the terminal shows instead, which is past what a
/// full history of a build's output takes as a screen, at 200 columns, about 1 MiB. A guess, not measured, to check
/// against a terminal with a colour in every cell, whose screen can be far larger.
pub(super) const BEHIND_BYTES: usize = 4 * 1024 * 1024;

/// The most updates it holds, frames of output among them, so a client too far behind on small updates, such as
/// another client's new sizes, is sent what the terminal shows too.
pub(super) const BEHIND_UPDATES: usize = 1024;

/// Updates held to send, in the order they came. Output is joined onto the output held last, up to
/// [`JOINED_OUTPUT_BYTES`], so a program that writes a line at a time isn't sent a frame for each. Joined, it means what
/// the two did, since a sequence can be split between one and the next.
#[derive(Debug, Default)]
pub(super) struct Backlog {
    updates: VecDeque<TerminalUpdate>,
    /// How much output it holds.
    output: usize,
}

impl Backlog {
    /// Holds `update`, after everything held already.
    pub(super) fn hold(&mut self, update: TerminalUpdate) {
        let TerminalUpdate::Output { output } = update else {
            self.updates.push_back(update);
            return;
        };
        self.output += output.len();
        let unjoined = match self.updates.back_mut() {
            Some(last) => join(last, output).err(),
            None => Some(output),
        };
        if let Some(output) = unjoined {
            self.updates.push_back(TerminalUpdate::Output { output });
        }
    }

    /// Takes the oldest update it holds, to send.
    pub(super) fn take_oldest(&mut self) -> Option<TerminalUpdate> {
        let oldest = self.updates.pop_front()?;
        if let TerminalUpdate::Output { output } = &oldest {
            self.output -= output.len();
        }
        Some(oldest)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.updates.is_empty()
    }

    /// Whether the client it holds for is too far behind, past [`BEHIND_BYTES`] or [`BEHIND_UPDATES`], and is sent what
    /// the terminal shows instead.
    pub(super) fn too_far_behind(&self) -> bool {
        self.output > BEHIND_BYTES || self.updates.len() > BEHIND_UPDATES
    }
}

/// Joins `more` onto `last` if it's output too, and the two come to no more than [`JOINED_OUTPUT_BYTES`]. Gives `more`
/// back otherwise.
fn join(last: &mut TerminalUpdate, more: TerminalBytes) -> Result<(), TerminalBytes> {
    let TerminalUpdate::Output { output } = last else { return Err(more) };
    if output.len() + more.len() > JOINED_OUTPUT_BYTES {
        return Err(more);
    }
    let mut joined = std::mem::replace(output, TerminalBytes::new(Vec::new())).into_vec();
    joined.extend_from_slice(more.as_slice());
    *output = TerminalBytes::new(joined);
    Ok(())
}
