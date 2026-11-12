//! One hub's records, kept current by following its feed, so a client only reads them.

use parking_lot::Mutex;
use protocol::client::HubNotice;
use protocol::model::{Record, Records, SessionId};

use super::follow::{Follow, FollowError, Source};
use super::{Client, ClientError, Result, Subscription, SubscriptionItem};
use crate::records::HubRecords;

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

    /// Keeps the subagents of `session` from now on, listing them first. Asking again does nothing. Readers hear of
    /// them once they're listed, not when they're asked for, so a reader can ask each time it reads.
    pub fn follow_subagents(&self, session: &SessionId) {
        if self.follow.quietly(|records| records.subagents_mut().want(session.clone())) {
            self.follow.wake();
        }
    }

    /// Lets go of the subagents of `session`, once no view shows it, such as when its chat closes.
    pub fn unfollow_subagents(&self, session: &SessionId) {
        self.follow.update(|records| records.subagents_mut().forget(session));
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
        held.lock().subagents_mut().relist();
        list_subagents(client, held)?;
        Ok(subscription)
    }

    fn apply(&mut self, client: &Client, held: &Mutex<HubRecords>, item: SubscriptionItem) -> Result<bool> {
        Ok(match item {
            // The hub found that some notices went missing. A new list makes up for them.
            SubscriptionItem::Notice(HubNotice::Resync) => {
                let list = list_records(client)?;
                let changed = held.lock().apply_list(list);
                held.lock().subagents_mut().relist();
                list_subagents(client, held)? || changed
            }
            SubscriptionItem::Notice(notice) => held.lock().apply_notice(notice),
            SubscriptionItem::Event(_) | SubscriptionItem::Tail(_) | SubscriptionItem::Delta(_) => false,
        })
    }

    /// Lists the subagents of a session a view has just asked for.
    fn asked(&mut self, client: &Client, held: &Mutex<HubRecords>) -> Result<bool> {
        list_subagents(client, held)
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
            let repos = client.repo_list()?;
            // It kept nothing else.
            Ok(Records { repos, workspaces, sessions, comments, ..Records::default() })
        }
        listed => listed,
    }
}

/// Lists the subagents of each session a view asked for that hasn't been listed on this connection. Returns whether
/// that changed anything. The hub refusing one session, such as one it doesn't have, leaves only that one unlisted
/// until the next connection, so it can't stop the feed. Only losing the connection fails.
fn list_subagents(client: &Client, held: &Mutex<HubRecords>) -> Result<bool> {
    let mut changed = false;
    // Bound first, so the lock is let go before the loop takes it again.
    let to_list = held.lock().subagents().to_list();
    for session in to_list {
        // Listed before the lock is taken, as every list is.
        match client.session_subagents(&session) {
            Ok((list, _)) => changed |= held.lock().subagents_mut().apply_list(&session, list),
            Err(e) if e.is_unsupported() => {
                held.lock().subagents_mut().unkept();
                return Ok(true);
            }
            Err(ClientError::Api(_)) => held.lock().subagents_mut().skip(session),
            Err(e) => return Err(e),
        }
    }
    Ok(changed)
}
