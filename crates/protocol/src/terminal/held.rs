//! A terminal's updates held while the last frame of them is on its way, by a hub for a client, or by a worker for its
//! hub, so a slow reader has one frame of them on its way at a time.

use std::collections::VecDeque;

use super::{TerminalBytes, TerminalUpdate};

/// The most output one update carries where what's held is joined, about what a worker reads from a terminal at a
/// time, so whatever's sent behind it on a slow link waits for little.
pub const JOINED_OUTPUT_BYTES: usize = 64 * 1024;

/// Updates held to send, in the order they came. Output is joined onto the output held last, up to
/// [`JOINED_OUTPUT_BYTES`], so a program that writes a line at a time isn't sent a frame for each. Joined, it means what
/// the two did, since a sequence can be split between one and the next.
#[derive(Debug, Default)]
pub struct HeldUpdates {
    updates: VecDeque<TerminalUpdate>,
    /// How much output it holds.
    output: usize,
    /// Whether output can be joined onto the update held last, which it can't once [`HeldUpdates::fence`] is called.
    joinable: bool,
}

impl HeldUpdates {
    /// Holds `update`, after everything held already. True if it takes a place of its own, rather than joining the
    /// last.
    pub fn hold(&mut self, update: TerminalUpdate) -> bool {
        let joinable = std::mem::replace(&mut self.joinable, true);
        let TerminalUpdate::Output { output } = update else {
            self.updates.push_back(update);
            return true;
        };
        self.output += output.len();
        let unjoined = match self.updates.back_mut() {
            Some(last) if joinable => join(last, output).err(),
            _ => Some(output),
        };
        let Some(output) = unjoined else { return false };
        self.updates.push_back(TerminalUpdate::Output { output });
        true
    }

    /// Stops what's held next from joining what's held already, for a sender that has to tell the two apart, as a
    /// worker does a screen it takes in one and not the other.
    pub fn fence(&mut self) {
        self.joinable = false;
    }

    /// Takes the oldest update it holds, to send.
    pub fn take_oldest(&mut self) -> Option<TerminalUpdate> {
        let next = self.updates.pop_front()?;
        if let TerminalUpdate::Output { output } = &next {
            self.output -= output.len();
        }
        Some(next)
    }

    pub fn len(&self) -> usize {
        self.updates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.updates.is_empty()
    }

    /// How many bytes of output it holds.
    pub fn output_bytes(&self) -> usize {
        self.output
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
