//! Starting and stopping the subscriptions a `Subscribe` says.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use protocol::client::github::MOST_REFERENCES_SHOWN;
use protocol::client::subscriptions::{Follow, Refused, Subscribe, Subscribed, Subscription};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::Level;

use super::{hub_level, reference_level, session_level, workspace_level};
use crate::hub::{Followed, Hub};
use crate::server::Connection;
use crate::server::sends::Sends;
use crate::server::subscribing::{self, taken, unsubscribe};
use crate::server::terminals::Watching;

/// What feeds one subscription, until the connection goes.
pub(super) type Feeding = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Stops each subscription `asked` stops, then starts each it starts, in order, and says which it couldn't start.
pub(in crate::server) async fn subscribe(hub: &Arc<Hub>, connection: &Connection, asked: Subscribe) -> Subscribed {
    let Subscribe { stop, start } = asked;
    for subscription_id in stop {
        unsubscribe(connection, subscription_id);
    }
    let mut refused = Vec::new();
    for Subscription { subscription_id, follow, github_observed } in start {
        if let Err(error) = start_one(hub, connection, subscription_id, follow, github_observed).await {
            refused.push(Refused { subscription_id, error });
        }
    }
    Subscribed { refused }
}

/// Starts subscription `subscription_id`, unless it already follows the same, as when a request is sent again.
async fn start_one(
    hub: &Arc<Hub>,
    connection: &Connection,
    subscription_id: u64,
    follow: Follow,
    github_observed: bool,
) -> Result<(), ApiError> {
    let level = follow.level();
    {
        let subscriptions = connection.subscriptions.lock();
        match subscriptions.get(&subscription_id) {
            Some(subscription) if subscription.level.as_ref() == Some(&level) => return Ok(()),
            Some(_) => return Err(taken(subscription_id)),
            None => {}
        }
        let references = subscriptions.values().filter(|s| matches!(s.level, Some(Level::Reference { .. }))).count();
        if matches!(level, Level::Reference { .. }) && references >= MOST_REFERENCES_SHOWN {
            let message = format!("over {MOST_REFERENCES_SHOWN} issues and pull requests followed on one connection");
            return Err(ApiError::new(ErrorCode::InvalidRequest, message));
        }
    }
    let sends = Sends::at(level.clone(), github_observed);
    let (followed, feeding) = feeding(hub, connection, subscription_id, follow, sends).await?;
    let mut subscriptions = connection.subscriptions.lock();
    // Another kind of request, which doesn't wait its turn as this does, may have taken the ID meanwhile.
    if subscriptions.contains_key(&subscription_id) {
        return Err(taken(subscription_id));
    }
    let task = tokio::spawn(feeding).abort_handle();
    let subscription = subscribing::Subscription::new(hub, connection, subscription_id, [followed], task);
    subscriptions.insert(subscription_id, subscription.of_level(level));
    Ok(())
}

/// What `follow` follows, as the hub counts it, and what feeds it, in the forms of what GitHub says that `forms` has it
/// sent, once the hub has what that needs: refused with `NotFound` for what it doesn't have, and `Unsupported` for a
/// level, or a place in a log, it doesn't know.
async fn feeding(
    hub: &Arc<Hub>,
    connection: &Connection,
    subscription_id: u64,
    follow: Follow,
    sends: Sends,
) -> Result<(Followed, Feeding), ApiError> {
    let frames = connection.frames.clone();
    Ok(match follow {
        Follow::Hub => (Followed::Hub, hub_level::feeding(hub, sends, subscription_id, frames).await?),
        Follow::Workspace { id } => {
            let feeding = workspace_level::feeding(hub, id.clone(), sends, subscription_id, frames).await?;
            (Followed::Workspace(id), feeding)
        }
        Follow::Session { id, from } => {
            let feeding = session_level::feeding(hub, id.clone(), from, sends, subscription_id, frames).await?;
            (Followed::Session(id), feeding)
        }
        Follow::Terminal { id } => {
            let watcher = hub.terminal_watch(&id)?;
            let feeding = Watching { subscription_id, frames }.watch(watcher);
            (Followed::Terminal(id), Box::pin(feeding))
        }
        Follow::Reference { reference } => {
            let feeding = reference_level::feeding(hub, reference.clone(), sends, subscription_id, frames).await?;
            (Followed::Reference(reference), feeding)
        }
        Follow::Unknown => return Err(unknown("a level")),
    })
}

/// The refusal of what a newer client follows, which this hub doesn't know.
pub(super) fn unknown(what: &str) -> ApiError {
    ApiError::new(ErrorCode::Unsupported, format!("{what} this hub doesn't know"))
}
