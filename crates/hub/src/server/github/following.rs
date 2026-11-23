//! Starting a subscription to what GitHub says of what a client shows, and replacing what it shows.

use std::sync::Arc;

use protocol::client::github::{GithubShown, GithubSubscribe, MOST_REFERENCES_SHOWN};
use protocol::client::{ApiError, ErrorCode};
use tokio::sync::watch;

use super::pumping::pump;
use crate::hub::{Followed, Hub};
use crate::server::Connection;
use crate::server::subscribing::{Served, taken};

/// Starts feeding a subscription to what GitHub says of what `subscribe` shows, or, for one that's already fed, takes
/// it to show that from now on.
pub(in crate::server) fn follow(
    hub: &Arc<Hub>,
    connection: &Connection,
    subscribe: GithubSubscribe,
) -> Result<(), ApiError> {
    let GithubSubscribe { subscription_id, shown } = subscribe;
    if shown.references.len() > MOST_REFERENCES_SHOWN {
        let message =
            format!("{} issues and pull requests shown at once, over {MOST_REFERENCES_SHOWN}", shown.references.len());
        return Err(ApiError::new(ErrorCode::InvalidRequest, message));
    }
    // Held throughout, as `add_subscription` holds it, so two requests can't both take one subscription ID.
    let mut subscriptions = connection.subscriptions.lock();
    if let Some(subscription) = subscriptions.get_mut(&subscription_id) {
        let Some(showing) = &subscription.shown else { return Err(taken(subscription_id)) };
        subscription.following.replace(followed(&shown));
        showing.send_replace(shown);
        return Ok(());
    }
    let followed = followed(&shown);
    let (sender, receiver) = watch::channel(shown);
    let task = tokio::spawn(pump(hub.clone(), receiver, subscription_id, connection.frames.clone())).abort_handle();
    let subscription = Served::new(hub, followed, task).showing(sender);
    subscriptions.insert(subscription_id, subscription);
    Ok(())
}

/// What a client that shows `shown` follows: each workspace whose pull request it shows in full, and each issue and
/// pull request it shows as a chip. The workspaces whose pull requests it shows on their marks say only what it's sent,
/// since a client that follows the hub has every workspace's pull request read again once the cache forgets it.
fn followed(shown: &GithubShown) -> Vec<Followed> {
    let closely = shown.closely.iter().cloned().map(Followed::Workspace);
    closely.chain(shown.references.iter().cloned().map(Followed::Reference)).collect()
}
