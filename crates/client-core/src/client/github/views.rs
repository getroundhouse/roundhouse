//! The handles a client has on what one hub says of GitHub: the feed, and each view's part of it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use protocol::model::{
    ObservationKey, PullRequestFound, ReferenceAsk, ReferenceRead, RepositoryUnseen, Workspace, WorkspaceId,
};

use super::held::{GithubHeld, HeldObservations};
use super::source::GithubSource;
use crate::client::Client;
use crate::client::follow::{Following, Reach};
use crate::records::{HubRecords, Snapshot};

/// What one hub says of GitHub, of every workspace's own pull request and of the chips every view made from this shows,
/// kept current until every clone of this, and every view, has gone. A workspace's checks, history and stack come only
/// while it's followed at its own level, as [`crate::RecordsFeed::follow_workspace`] does. It carries on from a new connection given to
/// [`GithubFeed::reconnect`].
#[derive(Clone)]
pub struct GithubFeed {
    follow: Following<GithubHeld>,
    next_view: Arc<AtomicU64>,
    /// The copy of the hub's records that what the hub sends of GitHub as observations is held in, with every other
    /// observation.
    records: Reach<HubRecords>,
}

impl GithubFeed {
    /// Starts following, on a thread of its own, with no chip shown until a view says what it shows, reading and
    /// writing what's sent as observations in `records`, the copy of the same hub's records.
    pub(in crate::client) fn start(client: &Client, records: Reach<HubRecords>) -> GithubFeed {
        let written = records.clone();
        let follow = Following::start(client, GithubHeld::default(), move || GithubSource::new(written));
        GithubFeed { follow, next_view: Arc::new(AtomicU64::new(0)), records }
    }

    /// What `read` makes of what the copy of the hub's records holds of GitHub.
    fn read<T>(&self, read: impl FnOnce(&HeldObservations<'_>) -> T) -> T {
        let snapshot: Option<Snapshot> = self.records.read(HubRecords::snapshot);
        let of = |key: &ObservationKey| snapshot.as_ref().and_then(|snapshot| snapshot.observation(key));
        read(&HeldObservations { of: &of, hub_listed: snapshot.as_ref().is_some_and(Snapshot::hub_listed) })
    }

    /// Carries on on a new connection, following again the chips shown.
    pub fn reconnect(&self, client: &Client) {
        self.follow.reconnect(client);
    }

    /// A view's part of it, which shows nothing until it says.
    pub fn view(&self) -> GithubView {
        let (changed_tx, changed) = flume::bounded(1);
        let id = self.next_view.fetch_add(1, Ordering::Relaxed);
        self.follow.quietly(|held| held.add(id, changed_tx));
        GithubView { feed: self.clone(), id, changed }
    }

    /// What the hub said of `ask`, while a view shows it, or `None` until it has.
    pub fn reference(&self, ask: &ReferenceAsk) -> Option<ReferenceRead> {
        self.read(|observed| GithubHeld::reference(ask, observed))
    }

    /// `workspace`'s pull request, as the hub said. `None` until the hub has looked.
    pub fn pull_request(&self, workspace: &Workspace) -> Option<PullRequestFound> {
        self.read(|observed| GithubHeld::pull_request(&workspace.id, observed))
    }

    /// The repository GitHub last showed the default none of, where `workspace`'s branch's pull requests are, as the hub
    /// said, so what [`GithubFeed::pull_request`] says is only what was known before. `None` from
    /// a hub from before it said.
    pub fn unseen(&self, workspace: &Workspace) -> Option<RepositoryUnseen> {
        self.read(|observed| GithubHeld::unseen(&workspace.id, observed))
    }

    /// Whether the hub has said what it knows of `workspace`'s pull request, even that it hasn't looked, as it does
    /// once the cache it reads from has forgotten it.
    pub fn said_pull_request(&self, workspace: &Workspace) -> bool {
        self.read(|observed| GithubHeld::said_pull_request(&workspace.id, observed))
    }

    /// Takes in that the copy of the hub's records has taken in the observations of GitHub `keys` name, which `changed`
    /// says changed it, as [`GithubHeld::observed`] says.
    pub(in crate::client) fn observed(&self, keys: &[ObservationKey], changed: bool) {
        self.follow.quietly(|held| held.observed(keys, changed));
    }
}

/// What a view shows of what one hub says of GitHub, all of it, each time it says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShownOfGithub {
    /// The workspaces whose own pull requests it shows, as their marks in a sidebar do. The hub's level sends every
    /// workspace's, so showing one has the hub read nothing more, and the view is told when one changes.
    pub workspaces: Vec<WorkspaceId>,
    /// The issues and pull requests it shows as chips, by their numbers, which the hub reads and sends while one shows.
    pub references: Vec<ReferenceAsk>,
}

/// A view's part of what the hub says of GitHub: what it shows, and what tells it that some of that changed. What it
/// shows is let go of as it's dropped.
pub struct GithubView {
    feed: GithubFeed,
    id: u64,
    changed: flume::Receiver<()>,
}

impl GithubView {
    /// Takes `shown` to be what this view shows from now on, all of it, and tells the hub if that changes the chips any
    /// view shows.
    pub fn show(&self, shown: ShownOfGithub) {
        self.feed.follow.quietly(|held| held.show(self.id, shown));
        self.feed.follow.wake();
    }

    /// Says that what the hub said of something this view shows changed. It holds one signal at a time, so changes
    /// nobody has looked at yet gather into one.
    pub fn changed(&self) -> &flume::Receiver<()> {
        &self.changed
    }

    /// The feed this is a part of, to read what the hub said.
    pub fn feed(&self) -> &GithubFeed {
        &self.feed
    }
}

impl Drop for GithubView {
    fn drop(&mut self) {
        self.feed.follow.quietly(|held| held.hide(self.id));
        self.feed.follow.wake();
    }
}
