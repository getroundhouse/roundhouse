//! What a connection follows: each subscription `Subscribe` started, which has its entries in the hub's list of who
//! follows what while it lasts, and ending one.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode};
use tokio::task::AbortHandle;

use super::Connection;
use crate::hub::{Followed, Hub};

/// One subscription the hub serves a connection, which ends as it's dropped: its task stops, and its entries leave the
/// hub's list of who follows what.
pub(super) struct Served {
    /// What sends what it follows.
    task: AbortHandle,
    /// Its entries in the hub's list of who follows what, held until it's dropped.
    _following: crate::hub::Following,
    /// What it follows, so a start sent again can be told from one that takes its ID.
    pub(super) followed: Followed,
}

impl Served {
    /// A subscription fed by `task`, which follows `followed` from now on.
    pub(super) fn new(hub: &Arc<Hub>, followed: Followed, task: AbortHandle) -> Self {
        let following = hub.follow([followed.clone()]);
        Self { task, _following: following, followed }
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Ends a subscription. One that isn't there is already ended.
pub(super) fn unsubscribe(connection: &Connection, subscription_id: u64) {
    // Dropped once the lock is let go, since its entries leaving the hub's list may tell a worker.
    let ended = connection.subscriptions.lock().remove(&subscription_id);
    drop(ended);
}

/// The refusal of a subscription whose ID another subscription of the connection has.
pub(super) fn taken(subscription_id: u64) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, format!("subscription {subscription_id} already exists"))
}
