//! A notice as the feed tells it, and what it moves of its record, which says which subscriptions are sent it.

use std::sync::Arc;

use protocol::client::HubNotice;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
#[cfg(test)]
use tokio::sync::broadcast::error::TryRecvError;

/// A notice as the feed tells it: what it says, and what it moves.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Fed {
    pub(crate) notice: Arc<HubNotice>,
    pub(crate) moves: Moves,
}

/// What a notice moves of what a client holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Moves {
    /// What every client that follows its level reads, as every notice but one of the next does.
    Anything,
    /// Only the fields of a record that a client of a version from before an observation reads in its place, which the
    /// hub fills from the observation, as a copy of an account's record whose revision moved only since who the account
    /// is changed. So a subscription whose client reads the observation itself isn't sent it, as
    /// `crate::server::sends` decides.
    OldFormOnly,
}

/// What the feed tells, as one of the hub's own tasks, or a test, reads it: each notice, whatever it moves, since what
/// each subscription is sent is decided where it's sent it, in `crate::server::sends`.
pub(crate) struct Notices(pub(super) broadcast::Receiver<Fed>);

impl Notices {
    /// The next notice, as [`broadcast::Receiver::recv`] gives it.
    pub(crate) async fn recv(&mut self) -> Result<Arc<HubNotice>, RecvError> {
        self.0.recv().await.map(|fed| fed.notice)
    }

    /// The next notice told already, as [`broadcast::Receiver::try_recv`] gives it.
    #[cfg(test)]
    pub(crate) fn try_recv(&mut self) -> Result<Arc<HubNotice>, TryRecvError> {
        self.0.try_recv().map(|fed| fed.notice)
    }
}
