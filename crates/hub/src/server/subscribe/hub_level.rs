//! A subscription at the hub's level: where every record but a session's own stands, with the hub's observations, then
//! each notice of the hub's level.

use std::sync::Arc;

use protocol::client::ApiError;
use protocol::client::listed::Listed;
use protocol::model::Level;
use tokio::sync::mpsc;

use super::notices::{forward, send_listed};
use super::starting::Feeding;
use crate::hub::Hub;
use crate::server::{Closer, Outgoing};

/// What feeds a subscription at the hub's level, once the hub has listed its records, so one it can't list is refused.
pub(super) async fn feeding(
    hub: &Arc<Hub>,
    subscription_id: u64,
    frames: mpsc::Sender<Outgoing>,
    closer: Closer,
) -> Result<Feeding, ApiError> {
    // Before the list, so nothing that changes after it is missed.
    let feed = hub.subscribe_feed();
    let listed = hub.records_list().await?;
    let hub = hub.clone();
    Ok(Box::pin(async move {
        if !send_listed(&frames, subscription_id, Listed::Hub(Box::new(listed))).await {
            return;
        }
        let list = || async { Ok(Listed::Hub(Box::new(hub.records_list().await?))) };
        forward(feed, &Level::Hub, subscription_id, &frames, &closer, list).await;
    }))
}
