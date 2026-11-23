//! A subscription at the level of an issue or a pull request, as a chip draws it: what GitHub says of it, in whichever
//! form the subscription is sent, as `crate::server::sends` decides.

use std::sync::Arc;

use protocol::client::ApiError;
use protocol::client::github::GithubShown;
use protocol::client::listed::Listed;
use protocol::model::ReferenceAsk;
use tokio::sync::{mpsc, watch};

use super::notices::{forward, send_listed};
use super::starting::Feeding;
use crate::hub::Hub;
use crate::server::Outgoing;
use crate::server::github::pump;
use crate::server::sends::Sends;

/// What feeds a subscription that follows `reference`, in the form of what GitHub says that `sends` has it sent: its
/// observations, listed first and then each change, or `HubFrame::Github` of it once the hub has read it and as that
/// changes. Refused only where the hub's revision can't be read, as its observations are listed.
pub(super) async fn feeding(
    hub: &Arc<Hub>,
    reference: ReferenceAsk,
    sends: Sends,
    subscription_id: u64,
    frames: mpsc::Sender<Outgoing>,
) -> Result<Feeding, ApiError> {
    if !sends.github_observed() {
        let (shown, showing) = watch::channel(GithubShown { references: vec![reference], ..GithubShown::default() });
        let hub = hub.clone();
        return Ok(Box::pin(async move {
            // What it shows is held for as long as it's fed, since its pump ends once nothing can change it.
            let _shown = shown;
            pump(hub, showing, subscription_id, frames).await;
        }));
    }
    // Before the list, so nothing that changes after it is missed.
    let feed = hub.subscribe_feed();
    let listed = hub.reference_listed(&reference).await?;
    let hub = hub.clone();
    Ok(Box::pin(async move {
        if !send_listed(&frames, &sends, subscription_id, Listed::Reference(listed)).await {
            return;
        }
        let list = || async { Ok(Listed::Reference(hub.reference_listed(&reference).await?)) };
        forward(feed, &sends, subscription_id, &frames, list, |_| {}).await;
    }))
}
