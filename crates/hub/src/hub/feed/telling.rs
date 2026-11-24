//! How the hub tells of a change on its feed, and how what reads the feed follows it.

use protocol::client::{HubNotice, RecordNotice};
use tokio::sync::broadcast;

use super::{Fed, Moves, Notices};
use crate::hub::Hub;

impl Hub {
    /// What the feed tells from now on, each notice whatever it moves, for one of the hub's own tasks or a test.
    pub(crate) fn subscribe_feed(&self) -> Notices {
        Notices(self.feed.subscribe())
    }

    /// The same, with what each notice moves, for what sends a subscription what its client reads, as
    /// `crate::server::sends` decides.
    pub(crate) fn subscribe_fed(&self) -> broadcast::Receiver<Fed> {
        self.feed.subscribe()
    }

    /// Tells every client following the feed of `notice`, with each field of a record's that a client of a version
    /// from before an observation reads it in filled from the observation, as `crate::hub::observations::OldForm` says,
    /// as [`Feed::send`](super::Feed::send) says.
    pub(crate) fn notify(&self, notice: HubNotice) {
        self.feed.send(self.with_old_forms(notice), Moves::Anything);
    }

    /// Tells of `notice`, a copy of a record whose revision moved only since an observation whose old form it holds
    /// changed, as [`Moves::OldFormOnly`] says, so only a client that reads that form is sent it.
    pub(crate) fn notify_old_form(&self, notice: HubNotice) {
        self.feed.send(self.with_old_forms(notice), Moves::OldFormOnly);
    }

    /// `notice`, with the old forms of a copy of a record filled.
    fn with_old_forms(&self, notice: HubNotice) -> HubNotice {
        match RecordNotice::try_from(notice) {
            Ok(RecordNotice::Copy(mut record)) => {
                self.observations.fill(&mut record);
                record.into()
            }
            Ok(removed @ RecordNotice::Removed { .. }) => removed.into(),
            Err(notice) => notice,
        }
    }
}
