//! The handles a client has on what one hub says of GitHub: the feed, and each view's part of it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use protocol::client::github::GithubShown;
use protocol::model::{PullRequestFound, ReferenceAsk, ReferenceRead, Workspace};

use super::held::GithubHeld;
use super::source::GithubSource;
use crate::client::Client;
use crate::client::follow::Follow;

/// What one hub says of GitHub, of what every view made from this shows, kept current until every clone of this, and
/// every view, has gone. It carries on from a new connection given to [`GithubFeed::reconnect`].
#[derive(Clone)]
pub struct GithubFeed {
    follow: Follow<GithubHeld>,
    next_view: Arc<AtomicU64>,
}

impl GithubFeed {
    /// Starts following, on a thread of its own, with nothing shown until a view says what it shows.
    pub fn start(client: &Client) -> GithubFeed {
        let follow = Follow::start(client, GithubHeld::default(), GithubSource::default);
        GithubFeed { follow, next_view: Arc::new(AtomicU64::new(0)) }
    }

    /// Carries on on a new connection, telling the hub again what's shown.
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
        self.follow.read(|held| held.reference(ask))
    }

    /// `workspace`'s pull request, as the hub said while a view shows it. `None` until the hub has looked.
    pub fn pull_request(&self, workspace: &Workspace) -> Option<PullRequestFound> {
        self.follow.read(|held| held.pull_request(&workspace.id))
    }

    /// Whether the hub has said what it knows of `workspace`'s pull request while a view shows it, even that it hasn't
    /// looked, as it does once the cache it reads from has forgotten it. A hub too old to say never has, since what it
    /// keeps on the workspace's record is in a form this build doesn't read.
    pub fn said_pull_request(&self, workspace: &Workspace) -> bool {
        self.follow.read(|held| held.said_pull_request(&workspace.id))
    }

    /// Whether the hub is too old to say what GitHub says of what's shown, until a new connection.
    pub fn unsupported(&self) -> bool {
        self.follow.read(|held| held.unsupported)
    }
}

/// A view's part of what the hub says of GitHub: what it shows, and what tells it that some of that changed. What it
/// shows is let go of as it's dropped.
pub struct GithubView {
    feed: GithubFeed,
    id: u64,
    changed: flume::Receiver<()>,
}

impl GithubView {
    /// Takes `shown` to be what this view shows from now on, all of it, and tells the hub if that changes what any view
    /// shows.
    pub fn show(&self, shown: GithubShown) {
        self.feed.follow.quietly(|held| held.show(self.id, shown));
        self.feed.follow.wake();
    }

    /// Says that what the hub said of something this view shows changed, or that the hub turned out to be too old to
    /// say. It holds one signal at a time, so changes nobody has looked at yet gather into one.
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
