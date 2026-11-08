//! Keeping one hub's records current, on a thread of its own, so a client only reads them.

use std::sync::Arc;

use flume::TrySendError;
use parking_lot::Mutex;
use protocol::client::HubNotice;
use protocol::model::{Records, Rev};

use super::{Client, Result, Subscription, SubscriptionItem};
use crate::records::{HubRecords, Record};

/// One hub's records, kept current by following its feed until the connection ends or every clone of this
/// is dropped.
#[derive(Clone)]
pub struct RecordsFeed {
    records: Arc<Mutex<HubRecords>>,
    /// Says that the records changed. It holds one signal at a time, so changes nobody has looked at
    /// yet collect into one, and a slow reader never falls behind. It closes when the feed ends: the
    /// connection has gone, or listing again after missing notices failed.
    pub changed: flume::Receiver<()>,
    /// Dropped with this, which stops the thread.
    _stop: flume::Sender<()>,
}

impl RecordsFeed {
    /// Follows the feed, then lists, so nothing can change in between unseen. Fails if either fails.
    pub fn follow(client: &Client) -> Result<RecordsFeed> {
        let subscription = client.feed_subscribe()?;
        let mut records = HubRecords::default();
        records.apply_list(list_records(client)?);
        let records = Arc::new(Mutex::new(records));
        let (changed_tx, changed) = flume::bounded(1);
        let (stop, stopped) = flume::bounded(0);
        let follower = Follower { client: client.clone(), subscription, records: records.clone(), changed: changed_tx };
        std::thread::Builder::new().name("records feed".to_string()).spawn(move || follower.run(&stopped))?;
        Ok(RecordsFeed { records, changed, _stop: stop })
    }

    /// The records as they are now.
    pub fn read<T>(&self, read: impl FnOnce(&HubRecords) -> T) -> T {
        read(&self.records.lock())
    }

    /// Takes in a record a reply carried, so what's shown needn't wait for the feed to say the same.
    /// Returns whether it changed anything.
    pub fn take_reply(&self, record: Record) -> bool {
        self.records.lock().apply_record(record)
    }
}

struct Follower {
    client: Client,
    subscription: Subscription,
    records: Arc<Mutex<HubRecords>>,
    changed: flume::Sender<()>,
}

impl Follower {
    fn run(self, stopped: &flume::Receiver<()>) {
        loop {
            let next = flume::Selector::new()
                .recv(&self.subscription.items, std::result::Result::ok)
                .recv(stopped, |_| None)
                .wait();
            let changed = match next {
                // Fell behind, so some notices are missing. A new list makes up for them.
                Some(SubscriptionItem::Notice(HubNotice::Resync)) => match list_records(&self.client) {
                    Ok(list) => self.records.lock().apply_list(list),
                    Err(_) => return,
                },
                Some(SubscriptionItem::Notice(notice)) => self.records.lock().apply_notice(notice),
                Some(SubscriptionItem::Event(_) | SubscriptionItem::Tail(_) | SubscriptionItem::Delta(_)) => false,
                Some(SubscriptionItem::Disconnected) | None => return,
            };
            if changed && let Err(TrySendError::Disconnected(())) = self.changed.try_send(()) {
                return;
            }
        }
    }
}

/// Every record at once. A hub from before that request is asked for each kind instead, which gives no
/// revisions, and then the order of one connection keeps the records right.
fn list_records(client: &Client) -> Result<Records> {
    match client.records_list() {
        Err(e) if e.is_unsupported() => {
            let workspaces = client.workspace_list(true)?;
            let (mut sessions, mut comments) = (Vec::new(), Vec::new());
            for workspace in &workspaces {
                sessions.extend(client.session_list(&workspace.id)?);
                comments.extend(client.comment_list(&workspace.id)?);
            }
            Ok(Records { repos: client.repo_list()?, workspaces, sessions, comments, rev: Rev::default() })
        }
        listed => listed,
    }
}
