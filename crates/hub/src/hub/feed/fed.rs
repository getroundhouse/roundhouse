//! What the feed tells, as a reader of it takes it.

use std::sync::Arc;

use protocol::client::HubNotice;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
#[cfg(test)]
use tokio::sync::broadcast::error::TryRecvError;

/// What the feed tells, as a subscription, one of the hub's own tasks, or a test, reads it: each notice, of every
/// level, since what each subscription is sent is decided where it's sent it, by its level.
pub(crate) struct Notices(pub(super) broadcast::Receiver<Arc<HubNotice>>);

impl Notices {
    /// The next notice, as [`broadcast::Receiver::recv`] gives it.
    pub(crate) async fn recv(&mut self) -> Result<Arc<HubNotice>, RecvError> {
        self.0.recv().await
    }

    /// The next notice told already, as [`broadcast::Receiver::try_recv`] gives it.
    #[cfg(test)]
    pub(crate) fn try_recv(&mut self) -> Result<Arc<HubNotice>, TryRecvError> {
        self.0.try_recv()
    }
}

#[cfg(test)]
impl From<broadcast::Receiver<Arc<HubNotice>>> for Notices {
    fn from(receiver: broadcast::Receiver<Arc<HubNotice>>) -> Self {
        Self(receiver)
    }
}
