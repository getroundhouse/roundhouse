use super::super::follow::{FollowError, Following, Reach, Source};
use super::super::workspace_level::{FollowedWorkspace, Workspaces};
use super::super::{ClientError, Connection, GithubFeed, Result, Subscription, SubscriptionItem};
use crate::Agents;
use crate::records::{HubRecords, Snapshot};
use parking_lot::Mutex;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::Follow;
use protocol::model::WorkspaceId;

/// One hub's records, and what GitHub says of each workspace's own pull request, kept current by following the hub, at
/// its level, until every clone of this has gone. It carries on from a new connection given to
/// [`RecordsFeed::reconnect`].
#[derive(Clone)]
pub struct RecordsFeed {
    pub(super) follow: Following<HubRecords>,
    /// What follows the chips views show, into the same copy.
    github: GithubFeed,
    /// What follows the workspaces followed at their own level, into the same copy.
    workspaces: Workspaces,
}

impl RecordsFeed {
    /// Starts following, on a thread of its own: follows the hub, and is caught up once the hub has said where its
    /// records stand.
    ///
    /// Every record a reply on `connection` carries is taken in too, as it comes, so whoever made the call needn't hand
    /// it on, and what's shown needn't wait for the hub to say the same. So are the records of each session an
    /// [`OpenSession`](crate::OpenSession) handed this follows, as its subscription sends them.
    pub fn start(connection: &Connection) -> RecordsFeed {
        RecordsFeed::started(Some(connection))
    }

    /// As [`RecordsFeed::start`], on `connection`, or with none until one is given to [`RecordsFeed::reconnect`], as a
    /// hub's are before its link has connected.
    pub(crate) fn started(connection: Option<&Connection>) -> RecordsFeed {
        let records = Following::unstarted(HubRecords::default());
        let github = GithubFeed::start(connection, records.reach());
        let workspaces = Workspaces::start(connection, records.reach());
        let follow = records.start(connection, || RecordsSource);
        let feed = RecordsFeed { follow, github, workspaces };
        if let Some(connection) = connection {
            connection.inner.folding.reach(&feed.follow);
        }
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

    /// Takes in `record` as though the hub had told of it, for a test whose fake hub holds what never changes.
    #[cfg(any(test, feature = "testing"))]
    pub fn take_record(&self, record: protocol::model::Record) {
        self.follow.update(|records| records.apply_record(record));
    }

    /// Takes in that the record `record` is gone as of `rev`, as though the hub had told of it, as
    /// [`RecordsFeed::take_record`] takes in a record.
    #[cfg(any(test, feature = "testing"))]
    pub fn take_removal(&self, record: protocol::model::RecordKey, rev: protocol::model::Rev) {
        self.follow.update(|records| records.apply_notice(protocol::client::HubNotice::Removed { record, rev }));
    }

    /// Takes in a copy of `observation` as though the hub had told of it, as [`RecordsFeed::take_record`] does a record.
    #[cfg(any(test, feature = "testing"))]
    pub fn take_observation(&self, observation: protocol::model::Observation) {
        self.follow.update(|records| records.apply_notice(protocol::client::HubNotice::Observation(observation)));
    }

    /// Carries on on a new connection, which takes in what replies on it carry, as [`RecordsFeed::start`] says, and so
    /// does what the same hub says of GitHub, and following each workspace followed at its own level. The connection
    /// may be to another hub, or to one whose revisions started again, so the first list of each level on it replaces
    /// what's held of that level, as [`HubRecords::connected_again`] says. Call it before anything that follows a session
    /// carries on on `connection`, since what that takes in goes into the same copy.
    pub(crate) fn reconnect(&self, connection: &Connection) {
        self.follow.quietly(HubRecords::connected_again);
        connection.inner.folding.reach(&self.follow);
        self.follow.reconnect(connection);
        self.github.reconnect(connection);
        self.workspaces.reconnect(connection);
    }

    /// Follows workspace `id` at its own level, as [`FollowedWorkspace`] says, until the handle this returns is dropped,
    /// and every other kept on it. A view that shows what only a workspace's own pane draws keeps one while it shows
    /// it.
    pub fn follow_workspace(&self, id: &WorkspaceId) -> FollowedWorkspace {
        self.workspaces.follow(id)
    }

    /// A handle on the copy of the hub's records that doesn't keep it followed, for what takes records into it that come
    /// another way, as a session's subscription sends its own.
    pub(in crate::client) fn reach(&self) -> Reach<HubRecords> {
        self.follow.reach()
    }

    /// What follows the chips views show, for a view to say which it shows.
    pub fn github(&self) -> GithubFeed {
        self.github.clone()
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

    /// Follows the hub, and takes in where its records stand, which the hub sends first, before it returns, so it's
    /// caught up once it has.
    fn resume(&mut self, connection: &Connection, held: &Mutex<HubRecords>) -> Result<Subscription> {
        let subscription = connection.follow(Follow::Hub)?;
        // Asked for on each connection, since it may be to another hub, and taken in with the records, so a view that
        // has a chat's record has what its agent offers.
        let agents = Agents::read(connection)?;
        held.lock().set_agents(agents);
        for item in &subscription.items {
            if let SubscriptionItem::Listed(listed) = &item
                && let Listed::Hub(_) = **listed
            {
                self.apply(connection, held, item)?;
                return Ok(subscription);
            }
            self.apply(connection, held, item)?;
        }
        // Its channel closed before the list came, as when the connection goes.
        Err(ClientError::Disconnected)
    }

    fn apply(&mut self, _: &Connection, held: &Mutex<HubRecords>, item: SubscriptionItem) -> Result<bool> {
        Ok(match item {
            SubscriptionItem::Listed(listed) => match *listed {
                Listed::Hub(records) => held.lock().apply_list(*records),
                // Of another level, which this doesn't follow.
                Listed::Session(_) | Listed::Workspace(_) | Listed::Reference(_) | Listed::Unknown => false,
            },
            SubscriptionItem::Notice(notice) => held.lock().apply_notice(*notice),
            SubscriptionItem::Event(_)
            | SubscriptionItem::Tail(_)
            | SubscriptionItem::Delta(_)
            | SubscriptionItem::Terminal(_)
            | SubscriptionItem::TerminalPicture(_) => false,
        })
    }

    /// Everything it holds comes from the hub's level, which needs nothing asked.
    fn asked(&mut self, _: Option<&Connection>, _: &Mutex<HubRecords>, _: &dyn Fn()) -> Result<bool> {
        Ok(false)
    }
}
