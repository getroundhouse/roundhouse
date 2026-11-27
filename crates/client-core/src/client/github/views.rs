//! The handles a client has on the chips one hub's views show: the feed, and each view's part of it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use protocol::model::ReferenceAsk;

use super::held::GithubHeld;
use crate::client::Client;
use crate::client::at_level::LevelSource;
use crate::client::follow::{Following, Reach};
use crate::records::HubRecords;

/// The chips every view made from this shows, followed until every clone of this, and every view, has gone. What the
/// hub sends of each goes into the copy of the hub's records, which views read, as [`crate::Snapshot::reference_said`]
/// says, and hear of through, as [`crate::RecordsFeed::changed`] says. It carries on from a new connection given to
/// [`GithubFeed::reconnect`].
#[derive(Clone)]
pub struct GithubFeed {
    follow: Following<GithubHeld>,
    next_view: Arc<AtomicU64>,
}

impl GithubFeed {
    /// Starts following, on a thread of its own, with no chip shown until a view says what it shows, writing what's
    /// sent in `records`, the copy of the same hub's records.
    pub(in crate::client) fn start(connection: Option<&Client>, records: Reach<HubRecords>) -> GithubFeed {
        let follow =
            Following::start(connection, GithubHeld::default(), move || LevelSource::<ReferenceAsk>::new(records));
        GithubFeed { follow, next_view: Arc::new(AtomicU64::new(0)) }
    }

    /// Carries on on a new connection, following again the chips shown.
    pub(crate) fn reconnect(&self, client: &Client) {
        self.follow.reconnect(client);
    }

    /// A view's part of it, which shows nothing until it says.
    pub fn view(&self) -> GithubView {
        let id = self.next_view.fetch_add(1, Ordering::Relaxed);
        self.follow.quietly(|held| held.add(id));
        GithubView { feed: self.clone(), id }
    }
}

/// The chips a view shows, which are let go of as it's dropped.
pub struct GithubView {
    feed: GithubFeed,
    id: u64,
}

impl GithubView {
    /// Takes `chips`, the issues and pull requests it shows as chips, by their numbers, to be what this view shows from
    /// now on, all of them, and tells the hub if that changes the chips any view shows, which the hub reads and sends
    /// while one shows them.
    pub fn show(&self, chips: Vec<ReferenceAsk>) {
        self.feed.follow.quietly(|held| held.show(self.id, chips));
        self.feed.follow.wake();
    }
}

impl Drop for GithubView {
    fn drop(&mut self) {
        self.feed.follow.quietly(|held| held.hide(self.id));
        self.feed.follow.wake();
    }
}
