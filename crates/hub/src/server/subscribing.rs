//! Starting and ending what a connection follows: a session's log, which [`super::pump`] feeds, or the hub's feed of
//! its records, which is fed from here.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode, HubFrame, HubNotice};
use protocol::model::SessionId;
use tokio::sync::{broadcast, mpsc};
use tokio::task::AbortHandle;

use super::Connection;
use super::pump::{Following, pump};
use crate::hub::Hub;

/// Starts feeding a subscription to a session's log.
pub(super) async fn follow_log(
    hub: &Arc<Hub>,
    connection: &Connection,
    id: SessionId,
    subscription_id: u64,
    following: Following,
) -> Result<(), ApiError> {
    let session = hub.live_session(&id).await?;
    add_subscription(connection, subscription_id, || {
        let live = session.subscribe();
        let frames = connection.frames.clone();
        tokio::spawn(pump(hub.clone(), session.clone(), id, subscription_id, following, live, frames)).abort_handle()
    })
}

/// Starts feeding a subscription to the hub's feed.
pub(super) fn follow_feed(hub: &Hub, connection: &Connection, subscription_id: u64) -> Result<(), ApiError> {
    add_subscription(connection, subscription_id, || {
        tokio::spawn(pump_feed(hub.subscribe_feed(), subscription_id, connection.frames.clone())).abort_handle()
    })
}

/// Ends any kind of subscription. One that isn't there is already ended.
pub(super) fn unsubscribe(connection: &Connection, subscription_id: u64) {
    if let Some(subscription) = connection.subscriptions.lock().remove(&subscription_id) {
        subscription.abort();
    }
}

/// Holds the lock while `start` runs, so two requests can't both take one subscription ID.
fn add_subscription(
    connection: &Connection,
    subscription_id: u64,
    start: impl FnOnce() -> AbortHandle,
) -> Result<(), ApiError> {
    let mut subscriptions = connection.subscriptions.lock();
    if subscriptions.contains_key(&subscription_id) {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("subscription {subscription_id} already exists")));
    }
    subscriptions.insert(subscription_id, start());
    Ok(())
}

async fn pump_feed(mut feed: broadcast::Receiver<HubNotice>, subscription_id: u64, frames: mpsc::Sender<HubFrame>) {
    loop {
        let notice = match feed.recv().await {
            Ok(notice) => notice,
            Err(broadcast::error::RecvError::Lagged(_)) => HubNotice::Resync,
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if frames.send(HubFrame::Notice { subscription_id, notice }).await.is_err() {
            return;
        }
    }
}
