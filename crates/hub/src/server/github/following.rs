//! Starting a subscription to what GitHub says of what a client shows, and replacing what it shows.

use std::sync::Arc;

use protocol::client::github::{GithubShown, GithubSubscribe};
use protocol::client::{ApiError, ErrorCode};
use tokio::sync::watch;

use super::pumping::pump;
use crate::hub::Hub;
use crate::hub::github_reads::{ShowingEnds, ShownBy};
use crate::server::Connection;

/// The most issues and pull requests one subscription can show as chips, which is more than a window shows, so a
/// client that shows more is wrong.
const MOST_SHOWN: usize = 200;

/// A subscription to what GitHub says of what a client shows, which a client can tell it shows something else.
pub(in crate::server) struct Followed {
    shown: watch::Sender<GithubShown>,
    ends: ShowingEnds,
}

/// Starts feeding a subscription to what GitHub says of what `subscribe` shows, or, for one that's already fed, takes
/// it to show that from now on.
pub(in crate::server) fn follow(
    hub: &Arc<Hub>,
    connection: &Connection,
    subscribe: GithubSubscribe,
) -> Result<(), ApiError> {
    let GithubSubscribe { subscription_id, shown } = subscribe;
    if shown.references.len() > MOST_SHOWN {
        let message = format!("{} issues and pull requests shown at once, over {MOST_SHOWN}", shown.references.len());
        return Err(ApiError::new(ErrorCode::InvalidRequest, message));
    }
    let by = ShownBy { connection: connection.id, subscription: subscription_id };
    // Held throughout, as `add_subscription` holds it, so two requests can't both take one subscription ID.
    let mut subscriptions = connection.subscriptions.lock();
    let mut followed = connection.github.lock();
    if let Some(followed) = followed.get_mut(&subscription_id) {
        followed.ends.show(shown.clone());
        followed.shown.send_replace(shown);
        return Ok(());
    }
    if subscriptions.contains_key(&subscription_id) {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("subscription {subscription_id} already exists")));
    }
    let ends = hub.show_github(by, shown.clone());
    let (sender, receiver) = watch::channel(shown);
    let pumping = tokio::spawn(pump(hub.clone(), receiver, subscription_id, connection.frames.clone()));
    subscriptions.insert(subscription_id, pumping.abort_handle());
    followed.insert(subscription_id, Followed { shown: sender, ends });
    Ok(())
}
