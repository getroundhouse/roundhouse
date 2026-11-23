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

/// What feeds a subscription that follows `reference`, in the forms of what GitHub says that `sends` has it sent: its observations, listed first and
/// then each change, and `HubFrame::Github` of it once the hub has read it and as that changes. Refused only where the
/// hub's revision can't be read, as its observations are listed.
pub(super) async fn feeding(
    hub: &Arc<Hub>,
    reference: ReferenceAsk,
    sends: Sends,
    subscription_id: u64,
    frames: mpsc::Sender<Outgoing>,
) -> Result<Feeding, ApiError> {
    // Before the list, so nothing that changes after it is missed.
    let feed = hub.subscribe_feed();
    let forms = sends.forms();
    let listed = if forms.observations { Some(hub.reference_listed(&reference).await?) } else { None };
    let hub = hub.clone();
    Ok(Box::pin(async move {
        let observed = async {
            let Some(listed) = listed else { return std::future::pending().await };
            if !send_listed(&frames, subscription_id, Listed::Reference(listed)).await {
                return;
            }
            let list = || async { Ok(Listed::Reference(hub.reference_listed(&reference).await?)) };
            forward(feed, &sends, subscription_id, &frames, list, |_| {}).await;
        };
        let updated = async {
            if !forms.updates {
                return std::future::pending().await;
            }
            let (_shown, showing) =
                watch::channel(GithubShown { references: vec![reference.clone()], ..GithubShown::default() });
            // What it shows is held for as long as it's fed, since its pump ends once nothing can change it.
            pump(hub.clone(), showing, subscription_id, frames.clone()).await;
        };
        // Each ends only once the connection has gone, and with it the other.
        tokio::select! {
            () = observed => {}
            () = updated => {}
        }
    }))
}
