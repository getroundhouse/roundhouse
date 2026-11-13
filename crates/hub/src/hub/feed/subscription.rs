use parking_lot::Mutex;
use protocol::client::{HubNotice, RecordNotice};
use protocol::model::{RecordKey, Rev};
use std::collections::HashMap;
use tokio::sync::broadcast;
use tracing::trace;

/// Status changes are rare, so a client only overflows this if it has stopped reading.
const CAPACITY: usize = 256;

pub(in super::super) struct Feed {
    pub(super) sender: broadcast::Sender<HubNotice>,
    /// The revision of the newest copy sent of each record, or of its removal. It keeps one for each record the feed
    /// has told of since the hub started, since a late copy of one removed long ago is still older.
    pub(super) sent: Mutex<HashMap<RecordKey, Rev>>,
}

impl Feed {
    pub(in super::super) fn new() -> Self {
        Self { sender: broadcast::channel(CAPACITY).0, sent: Mutex::default() }
    }

    pub(in super::super) fn subscribe(&self) -> broadcast::Receiver<HubNotice> {
        self.sender.subscribe()
    }

    /// Tells every client following the feed of `notice`, unless it's a copy of a record no newer than one it has told
    /// of, or than that record's removal, so each copy of a record that a client hears is newer than the last. Several
    /// tasks can announce one record, each with a copy read at or after its own change, so two can carry the same copy,
    /// or finish in the other order, and a change made while another's copy was read is told of only by the newer
    /// copy. A client would drop an older copy anyway, as [`Rev::supersedes`] says. The check and the sending happen
    /// under one lock, so the order they're checked in is the order clients hear them.
    pub(in super::super) fn send(&self, notice: HubNotice) {
        let mut sent = self.sent.lock();
        let notice = match RecordNotice::try_from(notice) {
            Ok(of) => {
                let (newest, rev) = (sent.entry(of.key()).or_default(), of.rev());
                // Only a hub that doesn't keep revisions makes a copy without one, which is always sent, as a client
                // always takes it.
                if !rev.supersedes(*newest) {
                    trace!(rev = rev.get(), newest = newest.get(), "left out a copy no newer than one told of");
                    return;
                }
                *newest = rev.max(*newest);
                HubNotice::from(of)
            }
            // Not of one record, such as `Resync`.
            Err(notice) => notice,
        };
        // An error only means nobody is listening.
        let _ = self.sender.send(notice);
    }
}
