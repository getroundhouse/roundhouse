use super::super::follow::{Follow, FollowError, Source};
use super::super::{Client, ClientError, Result, Subscription, SubscriptionItem};
use crate::Agents;
use crate::records::{BySession, HubRecords, OfSession, Snapshot};
use parking_lot::Mutex;
use protocol::client::HubNotice;
use protocol::model::{Records, SessionId};

/// One hub's records, kept current until every clone of this has gone. It carries on from a new connection
/// given to [`RecordsFeed::reconnect`].
#[derive(Clone)]
pub struct RecordsFeed {
    pub(super) follow: Follow<HubRecords>,
}

impl RecordsFeed {
    /// Starts following, on a thread of its own: subscribes, then lists, so nothing can change in between
    /// unseen.
    ///
    /// Every record a reply on `client` carries is taken in too, as it comes, so whoever made the call needn't hand it
    /// on, and what's shown needn't wait for the feed to say the same.
    pub fn start(client: &Client) -> RecordsFeed {
        let feed = RecordsFeed { follow: Follow::start(client, HubRecords::default(), || RecordsSource) };
        client.inner.folding.reach(&feed.follow);
        feed
    }

    /// Says that something changed: the records, the connection, or an error to hear about. It holds one
    /// signal at a time, so changes nobody has looked at yet gather into one, and a slow reader never
    /// falls behind. Each signal goes to one receiver, so one reader listens, however many clones read.
    pub fn changed(&self) -> &flume::Receiver<()> {
        &self.follow.changed
    }

    /// The records as they are now, for a view to read and keep until they next change. It's made once for each
    /// change, however many views ask, so asking again with nothing changed costs a pointer.
    pub fn snapshot(&self) -> Snapshot {
        self.follow.read(HubRecords::snapshot)
    }

    /// Keeps the records of `session` that are listed by session, its subagents and its proposals, from now on,
    /// listing them first. Asking again does nothing. Readers hear of them once they're listed, not when they're asked
    /// for, so a reader can ask each time it reads.
    pub fn follow_session_records(&self, session: &SessionId) {
        if self.follow.quietly(|records| records.want_session(session)) {
            self.follow.wake();
        }
    }

    /// Lets go of the subagents and proposals of `session`, once no view shows it, such as when its chat closes.
    pub fn unfollow_session_records(&self, session: &SessionId) {
        self.follow.update(|records| records.forget_session(session));
    }

    /// Takes in `record` as though the hub had told of it, for a test whose fake hub holds what never changes.
    #[cfg(any(test, feature = "testing"))]
    pub fn take_record(&self, record: protocol::model::Record) {
        self.follow.update(|records| records.apply_record(record));
    }

    /// Carries on on a new connection, listing again to catch up, and taking in what replies on it carry, as
    /// [`RecordsFeed::start`] says.
    pub fn reconnect(&self, client: &Client) {
        client.inner.folding.reach(&self.follow);
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
        // Asked for on each connection, since it may be to another hub, and taken in with the records, so a view that
        // has a chat's record has what its agent offers.
        let agents = Agents::read(client)?;
        // Listed before the lock is taken, so nobody reading the records waits on the hub.
        let list = list_records(client)?;
        {
            let mut held = held.lock();
            held.set_agents(agents);
            held.apply_list(list);
        }
        held.lock().relist_sessions();
        list_by_session(client, held)?;
        Ok(subscription)
    }

    fn apply(&mut self, client: &Client, held: &Mutex<HubRecords>, item: SubscriptionItem) -> Result<bool> {
        Ok(match item {
            // The hub found that some notices went missing. A new list makes up for them.
            SubscriptionItem::Notice(notice) if matches!(*notice, HubNotice::Resync) => {
                let list = list_records(client)?;
                let changed = held.lock().apply_list(list);
                held.lock().relist_sessions();
                list_by_session(client, held)? || changed
            }
            SubscriptionItem::Notice(notice) => held.lock().apply_notice(*notice),
            SubscriptionItem::Event(_)
            | SubscriptionItem::Tail(_)
            | SubscriptionItem::Delta(_)
            | SubscriptionItem::Terminal(_)
            | SubscriptionItem::Github(_) => false,
        })
    }

    /// Lists the subagents and proposals of a session a view has just asked for, once there's a connection to list them
    /// on.
    fn asked(&mut self, connection: Option<&Client>, held: &Mutex<HubRecords>, _: &dyn Fn()) -> Result<bool> {
        let Some(client) = connection else { return Ok(false) };
        list_by_session(client, held)
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

/// Lists the subagents, the proposals and how the replies were heard of each session a view asked for that haven't
/// been listed on this connection. Returns whether that changed anything.
fn list_by_session(client: &Client, held: &Mutex<HubRecords>) -> Result<bool> {
    let subagents = list_each(client, held, HubRecords::subagents_mut, |client, session| {
        client.session_subagents(session).map(|(list, _)| list)
    })?;
    let proposals = list_each(client, held, HubRecords::proposals_mut, |client, session| {
        client.session_proposals(session).map(|(list, _)| list)
    })?;
    let heard = list_each(client, held, HubRecords::replies_heard_mut, |client, session| {
        client.session_replies_heard(session).map(|(list, _)| list)
    })?;
    Ok(subagents || proposals || heard)
}

/// Lists one kind of record, which `kept` says where the records keep, for each session that needs it, with `list`.
/// The hub refusing one session, such as one it doesn't have, leaves only that one unlisted until the next
/// connection, so it can't stop the feed. A hub from before the kind had records has none to list. Only losing the
/// connection fails.
fn list_each<R: OfSession>(
    client: &Client,
    held: &Mutex<HubRecords>,
    kept: fn(&mut HubRecords) -> &mut BySession<R>,
    list: fn(&Client, &SessionId) -> Result<Vec<R>>,
) -> Result<bool> {
    let mut changed = false;
    // Bound first, so the lock is let go before the loop takes it again.
    let to_list = kept(&mut held.lock()).to_list();
    for session in to_list {
        // Listed before the lock is taken, as every list is.
        match list(client, &session) {
            Ok(listed) => changed |= kept(&mut held.lock()).apply_list(&session, listed),
            Err(e) if e.is_unsupported() => {
                kept(&mut held.lock()).unkept();
                return Ok(true);
            }
            Err(ClientError::Api(_)) => kept(&mut held.lock()).skip(session),
            Err(e) => return Err(e),
        }
    }
    Ok(changed)
}
