//! What a terminal sends the hub, numbered in order, one frame at a time, holding what comes while the last is still
//! being written, so a hub that's slow to read, as one on a slow link is, has at most one of its frames on its way, and
//! what's sent behind it, such as replies and what agents say, waits for little. Output that comes while a frame is on
//! its way is joined, so a program that writes a line at a time isn't sent a frame for each.

use std::sync::Arc;

use protocol::model::TerminalId;
use protocol::terminal::{HeldUpdates, TerminalUpdate, UpdateIndex};
use protocol::worker::WorkerFrame;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::serve::Outbox;

/// How much output a terminal holds before it stops taking in what its program writes, so the program waits in its PTY,
/// as it would for a slow terminal, rather than here. A few of its reads, so the echo of a key waits behind little.
pub(super) const HELD_BYTES: usize = 256 * 1024;

pub(super) struct Sending {
    terminal: TerminalId,
    outbox: Arc<Outbox>,
    /// The `at` of the oldest update held, which is how many it has sent.
    sent: UpdateIndex,
    held: HeldUpdates,
    /// Room for the one frame that can be on its way to the hub.
    room: Arc<Semaphore>,
}

impl Sending {
    pub(super) fn new(terminal: TerminalId, outbox: Arc<Outbox>) -> Self {
        let (sent, held, room) = (UpdateIndex::new(0), HeldUpdates::default(), Arc::new(Semaphore::new(1)));
        Self { terminal, outbox, sent, held, room }
    }

    /// The `at` of the next update, for a screen taken now, which takes in everything held, and is followed by what's
    /// held from now on, which isn't joined to what's held already, since the screen takes in the one and not the other.
    pub(super) fn screen_taken(&mut self) -> UpdateIndex {
        self.held.fence();
        UpdateIndex::new(self.sent.get() + self.held.len() as u64)
    }

    /// Holds `update` to send, after everything held already, numbered after it, unless it's output joined onto the
    /// output held last.
    pub(super) fn hold(&mut self, update: TerminalUpdate) {
        self.held.hold(update);
    }

    /// Whether it holds as much output as it takes in before the program waits.
    pub(super) fn full(&self) -> bool {
        self.held.output_bytes() >= HELD_BYTES
    }

    pub(super) fn holds_any(&self) -> bool {
        !self.held.is_empty()
    }

    /// Room for the next frame, to wait for, alongside whatever else the terminal waits for.
    pub(super) fn room(&self) -> Arc<Semaphore> {
        self.room.clone()
    }

    /// Sends the oldest update held, holding `written` until it has been written.
    pub(super) async fn send(&mut self, written: OwnedSemaphorePermit) {
        let Some(update) = self.held.take_oldest() else { return };
        let at = self.sent;
        self.sent = at.next();
        let frame = WorkerFrame::Terminal { terminal: self.terminal.clone(), at, update };
        self.outbox.send_holding(frame, Some(written)).await;
    }

    /// Sends everything held, and then `last`, as the shell's end is.
    pub(super) async fn finish(&mut self, last: TerminalUpdate) {
        self.hold(last);
        while self.holds_any() {
            let Ok(written) = self.room.clone().acquire_owned().await else { return };
            self.send(written).await;
        }
    }
}
