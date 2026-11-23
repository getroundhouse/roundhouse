//! Starting and stopping the subscriptions a `Subscribe` says.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use protocol::client::github::{GithubShown, MOST_REFERENCES_SHOWN};
use protocol::client::subscriptions::{Follow, Refused, Subscribe, Subscribed, Subscription};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::Level;
use tokio::sync::watch;

use super::{hub_level, session_level};
use crate::hub::{Followed, Hub};
use crate::server::Connection;
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
    for Subscription { subscription_id, follow } in start {
        if let Err(error) = start_one(hub, connection, subscription_id, follow).await {
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
    let (followed, feeding) = feeding(hub, connection, subscription_id, follow).await?;
    let mut subscriptions = connection.subscriptions.lock();
    // Another kind of request, which doesn't wait its turn as this does, may have taken the ID meanwhile.
    if subscriptions.contains_key(&subscription_id) {
        return Err(taken(subscription_id));
    }
    let task = feeding.map(|feeding| tokio::spawn(feeding).abort_handle());
    let subscription = subscribing::Subscription::new(hub, connection, subscription_id, [followed], task);
    subscriptions.insert(subscription_id, subscription.of_level(level));
    Ok(())
}

/// What `follow` follows, as the hub counts it, and what feeds it, once the hub has what that needs: refused with
/// `NotFound` for what it doesn't have, and `Unsupported` for a level, or a place in a log, it doesn't know.
async fn feeding(
    hub: &Arc<Hub>,
    connection: &Connection,
    subscription_id: u64,
    follow: Follow,
) -> Result<(Followed, Option<Feeding>), ApiError> {
    let frames = connection.frames.clone();
    Ok(match follow {
        Follow::Hub => (Followed::Hub, Some(hub_level::feeding(hub, subscription_id, frames).await?)),
        Follow::Workspace { id } => {
            hub.workspace(&id).await?;
            (Followed::Workspace(id), None)
        }
        Follow::Session { id, from } => {
            let feeding = session_level::feeding(hub, id.clone(), from, subscription_id, frames).await?;
            (Followed::Session(id), Some(feeding))
        }
        Follow::Terminal { id } => {
            let watcher = hub.terminal_watch(&id)?;
            let feeding = Watching { subscription_id, frames }.watch(watcher);
            (Followed::Terminal(id), Some(Box::pin(feeding)))
        }
        Follow::Reference { reference } => {
            let (shown, showing) =
                watch::channel(GithubShown { references: vec![reference.clone()], ..GithubShown::default() });
            let pump = crate::server::github::pump(hub.clone(), showing, subscription_id, frames);
            // What it shows is held for as long as it's fed, since its pump ends once nothing can change it.
            let feeding = async move {
                let _shown = shown;
                pump.await;
            };
            (Followed::Reference(reference), Some(Box::pin(feeding)))
        }
        Follow::Unknown => return Err(unknown("a level")),
    })
}

/// The refusal of what a newer client follows, which this hub doesn't know.
pub(super) fn unknown(what: &str) -> ApiError {
    ApiError::new(ErrorCode::Unsupported, format!("{what} this hub doesn't know"))
}
