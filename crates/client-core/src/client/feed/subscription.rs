use super::super::follow::{Follow, FollowError, Source};
use super::super::github::{github_keys, notice_key, own_keys};
use super::super::{Client, ClientError, GithubFeed, Result, Subscription, SubscriptionItem};
use crate::Agents;
use crate::records::{HubRecords, Snapshot};
use parking_lot::Mutex;
use protocol::client::github::GithubUpdate;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::Follow as Level;

/// One hub's records, and what GitHub says of each workspace's own pull request, kept current by following the hub, at
/// its level, until every clone of this has gone. It carries on from a new connection given to
/// [`RecordsFeed::reconnect`].
#[derive(Clone)]
pub struct RecordsFeed {
    pub(super) follow: Follow<HubRecords>,
    /// What the same hub says of GitHub, which the hub's level sends of every workspace's own pull request.
    github: GithubFeed,
}

impl RecordsFeed {
    /// Starts following, on a thread of its own: follows the hub, and is caught up once the hub has said where its
    /// records stand.
    ///
    /// Every record a reply on `client` carries is taken in too, as it comes, so whoever made the call needn't hand it
    /// on, and what's shown needn't wait for the hub to say the same. So are the records of each session a view follows,
    /// as its subscription sends them.
    pub fn start(client: &Client) -> RecordsFeed {
        let records = Follow::unstarted(HubRecords::default());
        let github = GithubFeed::start(client, records.reach());
        let source = RecordsSource { github: github.clone() };
        let follow = records.start(client, move || source);
        let feed = RecordsFeed { follow, github };
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

    /// Takes in `record` as though the hub had told of it, for a test whose fake hub holds what never changes.
    #[cfg(any(test, feature = "testing"))]
    pub fn take_record(&self, record: protocol::model::Record) {
        self.follow.update(|records| records.apply_record(record));
    }

    /// Takes in a copy of `observation` as though the hub had told of it, as [`RecordsFeed::take_record`] does a record.
    #[cfg(any(test, feature = "testing"))]
    pub fn take_observation(&self, observation: protocol::model::Observation) {
        self.follow.update(|records| records.apply_notice(protocol::client::HubNotice::Observation(observation)));
    }

    /// Carries on on a new connection, which takes in what replies on it carry, as [`RecordsFeed::start`] says, and so
    /// does what the same hub says of GitHub.
    pub fn reconnect(&self, client: &Client) {
        client.inner.folding.reach(&self.follow);
        self.follow.reconnect(client);
        self.github.reconnect(client);
    }

    /// What the same hub says of GitHub, of every workspace's own pull request and of what views show.
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

struct RecordsSource {
    /// Where what the hub says of each workspace's own pull request goes.
    github: GithubFeed,
}

impl Source for RecordsSource {
    type Held = HubRecords;

    /// Follows the hub, and takes in where its records stand, which the hub sends first, before it returns, so it's
    /// caught up once it has.
    fn resume(&mut self, client: &Client, held: &Mutex<HubRecords>) -> Result<Subscription> {
        let subscription = client.follow(Level::Hub)?;
        // Asked for on each connection, since it may be to another hub, and taken in with the records, so a view that
        // has a chat's record has what its agent offers.
        let agents = Agents::read(client)?;
        held.lock().set_agents(agents);
        for item in &subscription.items {
            if let SubscriptionItem::Listed(listed) = &item
                && let Listed::Hub(_) = **listed
            {
                self.apply(client, held, item)?;
                return Ok(subscription);
            }
            self.apply(client, held, item)?;
        }
        // Its channel closed before the list came, as when the connection goes.
        Err(ClientError::Disconnected)
    }

    fn apply(&mut self, _: &Client, held: &Mutex<HubRecords>, item: SubscriptionItem) -> Result<bool> {
        Ok(match item {
            SubscriptionItem::Listed(listed) => match *listed {
                Listed::Hub(records) => {
                    let keys = github_keys(&records.observations);
                    let changed = held.lock().apply_list(*records);
                    self.github.relisted(&keys, changed);
                    changed
                }
                // Of another level, which this doesn't follow.
                Listed::Session(_) | Listed::Workspace(_) | Listed::Reference(_) | Listed::Unknown => false,
            },
            SubscriptionItem::Notice(notice) => {
                let key = notice_key(&notice);
                let changed = held.lock().apply_notice(*notice);
                if let Some(key) = key {
                    self.github.observed(&[key], changed);
                }
                changed
            }
            SubscriptionItem::Github(update) => {
                // The hub sends it in the form from before observations, so what's held of it as one goes.
                if let GithubUpdate::PullRequest { workspace, .. } = &*update {
                    let own = own_keys(workspace);
                    let forgot = held.lock().forget_observed(|key| own.contains(key));
                    self.github.take(*update);
                    return Ok(forgot);
                }
                self.github.take(*update);
                false
            }
            SubscriptionItem::Event(_)
            | SubscriptionItem::Tail(_)
            | SubscriptionItem::Delta(_)
            | SubscriptionItem::Terminal(_)
            | SubscriptionItem::TerminalPicture(_) => false,
        })
    }

    /// Everything it holds comes from the hub's level, which needs nothing asked.
    fn asked(&mut self, _: Option<&Client>, _: &Mutex<HubRecords>, _: &dyn Fn()) -> Result<bool> {
        Ok(false)
    }
}
