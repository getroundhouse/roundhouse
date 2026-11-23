//! The one way a client follows what a hub serves: `Subscribe`, which starts and stops what it follows, each thing under
//! an ID of its own, at one of the levels `docs/principles/serving.md` names. Every subscription starts and stops
//! here. One [`Subscription`] can follow several things, whose items come on one channel, so a source that follows
//! many, such as the chips a window shows, changes what it follows in one request.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use parking_lot::Mutex;

use protocol::client::github::GithubUpdate;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, LogFrom, Refused, Subscribe, Subscribed, Subscription as Start};
use protocol::client::{ClientRequest, Request as _};
use protocol::model::{Level, Records, Seq, SessionId};

use super::connection::{Inner, subscription_channel};
use super::{Client, ClientError, Ended, Result, SubscriptionItem};

/// Which channel each subscription ID's items go to, and each channel's sender, which is kept while its
/// [`Subscription`] lasts, even while it follows nothing, so its channel closes only once the subscription ends, falls
/// behind or loses its connection.
#[derive(Default)]
pub(super) struct Channels {
    of: HashMap<u64, u64>,
    senders: HashMap<u64, flume::Sender<SubscriptionItem>>,
}

impl Channels {
    /// The sender of the channel subscription `id`'s items go to.
    pub(super) fn sender(&self, id: u64) -> Option<(u64, &flume::Sender<SubscriptionItem>)> {
        let channel = *self.of.get(&id)?;
        self.senders.get(&channel).map(|sender| (channel, sender))
    }

    /// Ends `channel`, which closes once what's in it has been read. Returns the subscription IDs whose items went to it.
    pub(super) fn end(&mut self, channel: u64) -> Vec<u64> {
        self.senders.remove(&channel);
        let ids = self.of.iter().filter(|(_, of)| **of == channel).map(|(id, _)| *id).collect::<Vec<_>>();
        self.of.retain(|_, of| *of != channel);
        ids
    }

    /// Ends every channel, as the connection does.
    pub(super) fn clear(&mut self) {
        self.of.clear();
        self.senders.clear();
    }
}

/// What a client follows of a hub, on one connection, with the items of all of it on one channel. Dropping it tells the
/// hub to stop sending any of it.
pub struct Subscription {
    changing: Changing,
    pub items: flume::Receiver<SubscriptionItem>,
}

impl Subscription {
    /// Why it ended, once its channel has closed. `None` while it's still going.
    pub fn ended(&self) -> Option<Ended> {
        let connection = &self.changing.connection;
        if connection.lagged.lock().contains(&self.changing.channel) {
            Some(Ended::FellBehind)
        } else if *connection.disconnected.lock() {
            Some(Ended::Disconnected)
        } else {
            None
        }
    }

    /// What changes what it follows, for a source that follows many things, while it lasts.
    pub(super) fn changing(&self) -> Changing {
        self.changing.clone()
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let connection = &self.changing.connection;
        // In this order, so a delivery under way can't mark it as fallen behind after it's gone.
        let ids = connection.subscriptions.lock().end(self.changing.channel);
        connection.lagged.lock().remove(&self.changing.channel);
        self.changing.followed.lock().clear();
        if !ids.is_empty() {
            // Not waiting for the reply: this can run on any thread, and the reply doesn't matter.
            let _ = connection.send_only(Subscribe { stop: ids, start: Vec::new() }.into_request());
        }
    }
}

/// Changes what a [`Subscription`] follows, on its connection. Once the subscription has ended, it changes nothing.
#[derive(Clone)]
pub(super) struct Changing {
    channel: u64,
    /// Each thing it follows, by the ID the hub sends its frames under.
    followed: Arc<Mutex<Vec<(u64, Level)>>>,
    connection: Arc<Inner>,
}

impl Changing {
    /// Follows `wanted` from now on, in place of what it followed, stopping what it no longer wants and starting what's
    /// new in one request, with each start's own fields, such as where a session's log starts, as `wanted` says.
    /// Returns the starts the hub refused, which it follows nothing of until it's asked again.
    pub(super) fn follow_only(&self, client: &Client, wanted: Vec<Follow>) -> Result<Vec<Refused>> {
        let (stop, start) = {
            let followed = self.followed.lock();
            let stop = (followed.iter())
                .filter(|(_, level)| !wanted.iter().any(|follow| follow.level() == *level))
                .map(|(id, _)| *id)
                .collect::<Vec<_>>();
            let start = wanted.into_iter().filter(|follow| !followed.iter().any(|(_, level)| follow.level() == *level));
            (stop, start.collect::<Vec<_>>())
        };
        self.change(client, stop, start)
    }

    /// Stops following `stop`, and starts following `start`, in one request. Each start's items are taken from before
    /// the request goes, since they can come before its reply. Returns the starts the hub refused.
    fn change(&self, client: &Client, stop: Vec<u64>, start: Vec<Follow>) -> Result<Vec<Refused>> {
        if stop.is_empty() && start.is_empty() {
            return Ok(Vec::new());
        }
        let next_id = || self.connection.next_id.fetch_add(1, Ordering::Relaxed);
        // Each says it reads what GitHub says as observations, which a hub sends at each level where it can, and a hub
        // from before them ignores.
        let start_of = |follow| Start { github_observed: true, ..Start::new(next_id(), follow) };
        let starts = start.into_iter().map(start_of).collect::<Vec<_>>();
        let started = starts.iter().map(|start| (start.subscription_id, start.follow.level())).collect::<Vec<_>>();
        {
            let mut channels = self.connection.subscriptions.lock();
            if !channels.senders.contains_key(&self.channel) {
                return Err(ClientError::Disconnected);
            }
            for id in &stop {
                channels.of.remove(id);
            }
            for (id, _) in &started {
                channels.of.insert(*id, self.channel);
            }
        }
        self.followed.lock().retain(|(id, _)| !stop.contains(id));
        let asked = client.call(Subscribe { stop, start: starts });
        let mut channels = self.connection.subscriptions.lock();
        let refused = match asked {
            Ok(Subscribed { refused }) => refused,
            Err(e) => {
                for (id, _) in &started {
                    channels.of.remove(id);
                }
                return Err(e);
            }
        };
        // Ended meanwhile, so nothing it started is followed.
        let going = channels.senders.contains_key(&self.channel);
        let mut ended = Vec::new();
        {
            let mut followed = self.followed.lock();
            for (id, level) in started {
                let started = !refused.iter().any(|refused| refused.subscription_id == id);
                if going && started {
                    followed.push((id, level));
                } else {
                    channels.of.remove(&id);
                    ended.extend(started.then_some(id));
                }
            }
        }
        drop(channels);
        // What ended it told the hub to stop, but that can have reached the hub before the start did, so the start
        // would run with nothing to stop it. The hub stops nothing it isn't following, so stopping it again costs a
        // request.
        if !ended.is_empty() {
            let _ = self.connection.send_only(Subscribe { stop: ended, start: Vec::new() }.into_request());
        }
        Ok(refused)
    }
}

impl Client {
    /// Follows `follow`. Fails with the hub's refusal, as with `NotFound` for what it doesn't have.
    pub fn follow(&self, follow: Follow) -> Result<Subscription> {
        let (subscription, refused) = self.follow_all(vec![follow])?;
        match refused.into_iter().next() {
            Some(Refused { error, .. }) => Err(error.into()),
            None => Ok(subscription),
        }
    }

    /// Follows session `id`'s log from `from`, at the session's level, for what reads every event as it's recorded, such as
    /// the command line. The session's own records come first, as a [`SubscriptionItem::Listed`].
    pub fn follow_session(&self, id: &SessionId, from: Seq) -> Result<Subscription> {
        self.follow(Follow::Session { id: id.clone(), from: Some(LogFrom::Seq { seq: from }) })
    }

    /// Follows each of `follows`, with the items of all of them on one channel, and says which the hub refused.
    pub(super) fn follow_all(&self, follows: Vec<Follow>) -> Result<(Subscription, Vec<Refused>)> {
        let channel = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, items) = subscription_channel();
        self.inner.subscriptions.lock().senders.insert(channel, sender);
        let changing = Changing { channel, followed: Arc::default(), connection: self.inner.clone() };
        let subscription = Subscription { changing, items };
        let refused = subscription.changing.change(self, Vec::new(), follows)?;
        Ok((subscription, refused))
    }

    /// Every record and the hub's observations, as they stand now, read by following the hub until it says where it
    /// stands, for what reads them once, such as the command line.
    pub fn records(&self) -> Result<Records> {
        let subscription = self.follow(Follow::Hub)?;
        for item in &subscription.items {
            if let SubscriptionItem::Listed(listed) = item
                && let Listed::Hub(records) = *listed
            {
                return Ok(*records);
            }
        }
        Err(ClientError::Disconnected)
    }
}

impl SubscriptionItem {
    /// The level it's of, where it says, so a source can drop what was already on its way from a level it has stopped
    /// following: a list's or a notice's level, and for what's sent in the form from before observations, `GithubUpdate`,
    /// the level of what it's of, a chip's or a workspace's.
    pub(super) fn level(&self) -> Option<Level> {
        match self {
            SubscriptionItem::Github(update) => match &**update {
                GithubUpdate::Reference { reference, .. } => Some(Level::Reference { reference: reference.clone() }),
                GithubUpdate::PullRequest { workspace, .. } => Some(Level::Workspace { id: workspace.clone() }),
                GithubUpdate::Unknown => None,
            },
            SubscriptionItem::Listed(listed) => match &**listed {
                Listed::Workspace(observations) => Some(Level::Workspace { id: observations.workspace_id.clone() }),
                Listed::Reference(observations) => Some(Level::Reference { reference: observations.reference.clone() }),
                Listed::Hub(_) => Some(Level::Hub),
                Listed::Session(records) => Some(Level::Session { id: records.session_id.clone() }),
                Listed::Unknown => None,
            },
            SubscriptionItem::Notice(notice) => notice.level(),
            SubscriptionItem::Event(_)
            | SubscriptionItem::Tail(_)
            | SubscriptionItem::Delta(_)
            | SubscriptionItem::Terminal(_)
            | SubscriptionItem::TerminalPicture(_) => None,
        }
    }
}

/// Stops every subscription that `channel` is the channel of, as one whose reader fell behind, telling the hub.
pub(super) fn fell_behind(inner: &Inner, channel: u64) {
    // Marked first, so its reader never sees the channel close without knowing why. Ending it closes the channel once
    // what's in it has been read.
    inner.lagged.lock().insert(channel);
    let ids = inner.subscriptions.lock().end(channel);
    if !ids.is_empty() {
        let _ = inner.send_only(ClientRequest::Subscribe(Subscribe { stop: ids, start: Vec::new() }));
    }
}
