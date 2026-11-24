//! A subscription at the level of an issue or a pull request, as a chip draws it: what GitHub says of it.

use std::sync::Arc;

use protocol::client::ApiError;
use protocol::client::listed::{Listed, ReferenceObservations};
use protocol::model::{Level, ReferenceAsk};
use tokio::sync::mpsc;

use super::notices::{forward, send_listed};
use super::starting::Feeding;
use crate::hub::Hub;
use crate::server::{Closer, Outgoing};

/// What feeds a subscription that follows `reference`: its observations, listed first and then each change. Refused
/// only where the hub's revision can't be read, as its observations are listed.
pub(super) async fn feeding(
    hub: &Arc<Hub>,
    reference: ReferenceAsk,
    subscription_id: u64,
    frames: mpsc::Sender<Outgoing>,
    closer: Closer,
) -> Result<Feeding, ApiError> {
    // Before the list, so nothing that changes after it is missed.
    let feed = hub.subscribe_feed();
    let first = listed(hub, &reference).await?;
    let hub = hub.clone();
    Ok(Box::pin(async move {
        if !send_listed(&frames, subscription_id, Listed::Reference(first)).await {
            return;
        }
        let level = Level::Reference { reference: reference.clone() };
        let list = || async { Ok(Listed::Reference(listed(&hub, &reference).await?)) };
        forward(feed, &level, subscription_id, &frames, &closer, list).await;
    }))
}

/// Every observation of `reference`'s level, as a chip that follows it draws them.
async fn listed(hub: &Hub, reference: &ReferenceAsk) -> Result<ReferenceObservations, ApiError> {
    let (observations, rev) = hub.observations_at(&Level::Reference { reference: reference.clone() }).await?;
    Ok(ReferenceObservations { reference: reference.clone(), observations, rev })
}
