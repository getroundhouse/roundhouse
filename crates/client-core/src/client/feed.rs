//! One hub's records, kept current by following its feed, so a client only reads them.

use parking_lot::Mutex;
use protocol::client::HubNotice;
use protocol::model::{Records, Rev};

use super::follow::{Follow, FollowError, Source};
use super::{Client, Result, Subscription, SubscriptionItem};
use crate::records::{HubRecords, Record};

/// One hub's records, kept current until every clone of this has gone. It carries on from a new connection
/// given to [`RecordsFeed::reconnect`].
#[derive(Clone)]
pub struct RecordsFeed {
    follow: Follow<HubRecords>,
}

impl RecordsFeed {
    /// Starts following, on a thread of its own: subscribes, then lists, so nothing can change in between
    /// unseen.
    pub fn start(client: &Client) -> RecordsFeed {
        RecordsFeed { follow: Follow::start(client, HubRecords::default(), RecordsSource) }
    }

    /// Says that something changed: the records, the connection, or an error to hear about. It holds one
    /// signal at a time, so changes nobody has looked at yet gather into one, and a slow reader never
    /// falls behind. Each signal goes to one receiver, so one reader listens, however many clones read.
    pub fn changed(&self) -> &flume::Receiver<()> {
        &self.follow.changed
    }

    /// The records as they are now.
    pub fn read<T>(&self, read: impl FnOnce(&HubRecords) -> T) -> T {
        self.follow.read(read)
    }

    /// Takes in a record a reply carried, so what's shown needn't wait for the feed to say the same.
    pub fn take_reply(&self, record: Record) {
        self.follow.update(|records| records.apply_record(record));
    }

    /// Carries on on a new connection, listing again to catch up.
    pub fn reconnect(&self, client: &Client) {
        self.follow.reconnect(client);
    }

    /// True once it has caught up on a connection, and false from when that connection goes, or following
    /// fails, until it has caught up on a new one.
    pub fn connected(&self) -> bool {
        self.follow.connected()
    }

    pub fn take_errors(&self) -> Vec<FollowError> {
        self.follow.take_errors()
    }
}

struct RecordsSource;

impl Source for RecordsSource {
    type Held = HubRecords;

    fn resume(&mut self, client: &Client, held: &Mutex<HubRecords>) -> Result<Subscription> {
        let subscription = client.feed_subscribe()?;
        // Listed before the lock is taken, so nobody reading the records waits on the hub.
        let list = list_records(client)?;
        held.lock().apply_list(list);
        Ok(subscription)
    }

    fn apply(&mut self, client: &Client, held: &Mutex<HubRecords>, item: SubscriptionItem) -> Result<bool> {
        Ok(match item {
            // The hub found that some notices went missing. A new list makes up for them.
            SubscriptionItem::Notice(HubNotice::Resync) => {
                let list = list_records(client)?;
                held.lock().apply_list(list)
            }
            SubscriptionItem::Notice(notice) => held.lock().apply_notice(notice),
            SubscriptionItem::Event(_) | SubscriptionItem::Tail(_) | SubscriptionItem::Delta(_) => false,
        })
    }

    fn asked(&mut self, _: &Client, _: &Mutex<HubRecords>) -> Result<bool> {
        Ok(false)
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
