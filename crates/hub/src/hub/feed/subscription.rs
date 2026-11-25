use parking_lot::Mutex;
use protocol::client::{HubNotice, RecordNotice};
use protocol::model::{RecordKey, Rev};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::trace;

/// How many notices the feed holds for a receiver that hasn't taken them yet. Every level's notices share them, so a
/// receiver falls behind once this many of any level arrive while it waits, as a subscription does for room on a slow
/// connection, and then recovers as it says, as a `Subscribe` subscription is sent its level's list again. It's more
/// than a burst sends, such as GitHub's observer working out each workspace and chip in turn as the hub starts. Each
/// notice is shared, so the feed holds one copy of each of the last this many, and a receiver copies only what it sends.
const CAPACITY: usize = 256;

pub(in super::super) struct Feed {
    pub(super) sender: broadcast::Sender<Arc<HubNotice>>,
    /// The revision of the newest copy sent of each record, or of its removal. It keeps them for each record the feed
    /// has told of since the hub started, since a late copy of one removed long ago is still older.
    pub(super) sent: Mutex<HashMap<RecordKey, Rev>>,
}

impl Feed {
    pub(in super::super) fn new() -> Self {
        Self { sender: broadcast::channel(CAPACITY).0, sent: Mutex::default() }
    }

    pub(in super::super) fn subscribe(&self) -> broadcast::Receiver<Arc<HubNotice>> {
        self.sender.subscribe()
    }

    /// Tells every client following the feed of `notice`, unless it's a copy of a record no newer than one it has told
    /// of, or than that record's removal, so each copy of a record that a client hears is newer than the last. Several
    /// tasks can announce one record, each with a copy read at or after its own change, so two can carry the same copy,
    /// or finish in the other order, and a change made while another's copy was read is told of only by the newer copy.
    /// The check and the sending happen under one lock, so the order they're checked in is the order clients hear them.
    pub(in super::super) fn send(&self, notice: HubNotice) {
        let mut sent = self.sent.lock();
        let notice = match RecordNotice::try_from(notice) {
            Ok(of) => {
                let (newest, rev) = (sent.entry(of.key()).or_default(), of.rev());
                if rev <= *newest {
                    trace!(
                        name: "feed.copy_left_out",
                        rev = rev.get(),
                        newest = newest.get(),
                        "Left out a copy no newer than one told of"
                    );
                    return;
                }
                *newest = rev;
                HubNotice::from(of)
            }
            // Not of one record, such as `Resync`.
            Err(notice) => notice,
        };
        // An error only means nobody is listening.
        let _ = self.sender.send(Arc::new(notice));
    }
}
