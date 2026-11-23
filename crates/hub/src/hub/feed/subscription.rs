use parking_lot::Mutex;
use protocol::client::{HubNotice, RecordNotice};
use protocol::model::{RecordKey, Rev};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::trace;

use super::fed::{Fed, Moves};

/// How many notices the feed holds for a receiver that hasn't taken them yet. Every level's notices share them, so a
/// receiver falls behind once this many of any level arrive while it waits, as a subscription does for room on a slow
/// connection, and then recovers as it says, as a `Subscribe` subscription is sent its level's list again. It's more
/// than a burst sends, such as GitHub's observer working out each workspace and chip in turn as the hub starts. Each
/// notice is shared, so the feed holds one copy of each of the last this many, and a receiver copies only what it sends.
const CAPACITY: usize = 256;

pub(in super::super) struct Feed {
    pub(super) sender: broadcast::Sender<Fed>,
    /// The revisions of the newest copies sent of each record, or of its removal. It keeps them for each record the
    /// feed has told of since the hub started, since a late copy of one removed long ago is still older.
    pub(super) sent: Mutex<HashMap<RecordKey, Sent>>,
}

/// The revisions of the newest copies the feed has sent of one record.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Sent {
    /// Of any copy.
    newest: Rev,
    /// Of a copy that moves anything, or of the removal, which a client that reads an observation in place of the old
    /// form is sent, so it's compared with only these.
    moving_anything: Rev,
}

impl Feed {
    pub(in super::super) fn new() -> Self {
        Self { sender: broadcast::channel(CAPACITY).0, sent: Mutex::default() }
    }

    pub(in super::super) fn subscribe(&self) -> broadcast::Receiver<Fed> {
        self.sender.subscribe()
    }

    /// How many follow it.
    #[cfg(test)]
    pub(in super::super) fn receivers(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Tells every client following the feed of `notice`, which moves what `moves` says, unless it's a copy of a record
    /// no newer than one it has told of that a client sent it is sent too, or than that record's removal, so each copy
    /// of a record that a client hears is newer than the last. A copy that moves anything is compared with only those
    /// that do, so a client that isn't sent those that move an old form alone hears each copy that moves anything. One
    /// no newer than a copy that moves an old form alone is marked as told in it, so a client sent those isn't sent it,
    /// and hears each copy newer than the last too, as `crate::hub::observations::OldForm` makes two such copies when
    /// an announcement reads a record between an observation's change and the move of its record's revision.
    /// Several tasks can announce one record, each with a copy read at or after its own change, so two can carry the
    /// same copy, or finish in the other order, and a change made while another's copy was read is told of only by the
    /// newer copy. The check and the sending happen under one lock, so the order they're checked in is the order
    /// clients hear them.
    pub(in super::super) fn send(&self, notice: HubNotice, moves: Moves) {
        let mut sent = self.sent.lock();
        let mut told_in_old_form = false;
        let notice = match RecordNotice::try_from(notice) {
            Ok(of) => {
                let (sent, rev) = (sent.entry(of.key()).or_default(), of.rev());
                let newest = match moves {
                    Moves::Anything => sent.moving_anything,
                    Moves::OldFormOnly => sent.newest,
                };
                // Only a hub that doesn't keep revisions makes a copy without one, which is always sent, as a client
                // always takes it.
                if !rev.supersedes(newest) {
                    trace!(rev = rev.get(), newest = newest.get(), "left out a copy no newer than one told of");
                    return;
                }
                told_in_old_form = !rev.supersedes(sent.newest);
                sent.newest = rev.max(sent.newest);
                if moves == Moves::Anything {
                    sent.moving_anything = rev.max(sent.moving_anything);
                }
                HubNotice::from(of)
            }
            // Not of one record, such as `Resync`.
            Err(notice) => notice,
        };
        // An error only means nobody is listening.
        let _ = self.sender.send(Fed { notice: Arc::new(notice), moves, told_in_old_form });
    }
}
