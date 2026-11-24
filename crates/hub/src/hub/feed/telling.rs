//! How the hub tells of a change on its feed, and how what reads the feed follows it.

use protocol::client::HubNotice;

use super::Notices;
use crate::hub::Hub;

impl Hub {
    /// What the feed tells from now on, for a subscription, one of the hub's own tasks, or a test.
    pub(crate) fn subscribe_feed(&self) -> Notices {
        Notices(self.feed.subscribe())
    }

    /// Tells every client following the feed of `notice`, as [`Feed::send`](super::Feed::send) says.
    pub(crate) fn notify(&self, notice: HubNotice) {
        self.feed.send(notice);
    }
}
