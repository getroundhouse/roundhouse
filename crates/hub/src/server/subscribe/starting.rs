//! Starting and stopping the subscriptions a `Subscribe` says.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use protocol::client::github::MOST_REFERENCES_SHOWN;
use protocol::client::subscriptions::{Follow, Refused, Subscribe, Subscribed, Subscription};
use protocol::client::{ApiError, ErrorCode};
use tokio::sync::oneshot;

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
    // `None` for a level from a newer client, which is refused once it's known not to take a taken ID.
    let followed = Followed::try_from(&follow.level()).ok();
    {
        let subscriptions = connection.subscriptions.lock();
        match subscriptions.get(&subscription_id) {
            Some(subscription) if followed.is_some() && subscription.followed == followed => return Ok(()),
            Some(_) => return Err(taken(subscription_id)),
            None => {}
        }
        let references = subscriptions.values().filter(|s| matches!(s.followed, Some(Followed::Reference(_)))).count();
        if matches!(followed, Some(Followed::Reference(_))) && references >= MOST_REFERENCES_SHOWN {
            let message = format!("over {MOST_REFERENCES_SHOWN} issues and pull requests followed on one connection");
            return Err(ApiError::new(ErrorCode::InvalidRequest, message));
        }
    }
    let Some(followed) = followed else { return Err(unknown("a level")) };
    let sends = Sends::at(followed.level(), github_observed);
    let Fed { feeding, listed } = feeding(hub, connection, subscription_id, follow, sends).await?;
    {
        let mut subscriptions = connection.subscriptions.lock();
        // Another kind of request, which doesn't wait its turn as this does, may have taken the ID meanwhile.
        if subscriptions.contains_key(&subscription_id) {
            return Err(taken(subscription_id));
        }
        let task = tokio::spawn(feeding).abort_handle();
        let subscription = subscribing::Served::new(hub, [followed.clone()], task);
        subscriptions.insert(subscription_id, subscription.of_subscribe(followed));
    }
    // Only an error once the subscription has stopped, as another kind of request can stop it, which leaves no list to
    // wait for.
    if let Some(listed) = listed {
        let _ = listed.await;
    }
    Ok(())
}

/// What feeds one subscription, and, at a level whose list is queued before the reply to the `Subscribe` that starts
/// it, what says that the list is queued.
struct Fed {
    feeding: Feeding,
    listed: Option<oneshot::Receiver<()>>,
}

impl From<Feeding> for Fed {
    fn from(feeding: Feeding) -> Self {
        Self { feeding, listed: None }
    }
}

/// What feeds what `follow` follows, sent as `sends` has it, once the hub has what that needs: refused with `NotFound`
/// for what it doesn't have, and `Unsupported` for a level, or a place in a log, it doesn't know.
async fn feeding(
    hub: &Arc<Hub>,
    connection: &Connection,
    subscription_id: u64,
    follow: Follow,
    sends: Sends,
) -> Result<Fed, ApiError> {
    let (frames, closer) = (connection.frames.clone(), connection.closer.clone());
    Ok(match follow {
        Follow::Hub => hub_level::feeding(hub, sends, subscription_id, frames, closer).await?.into(),
        Follow::Workspace { id } => {
            let (feeding, listed) = workspace_level::feeding(hub, id, sends, subscription_id, frames, closer).await?;
            Fed { feeding, listed: Some(listed) }
        }
        Follow::Session { id, from } => {
            session_level::feeding(hub, id, from, sends, subscription_id, frames, closer).await?.into()
        }
        Follow::Terminal { id } => {
            let watcher = hub.terminal_watch(&id)?;
            let feeding: Feeding = Box::pin(Watching { subscription_id, frames }.watch(watcher));
            feeding.into()
        }
        Follow::Reference { reference } => {
            reference_level::feeding(hub, reference, sends, subscription_id, frames, closer).await?.into()
        }
        Follow::Unknown => return Err(unknown("a level")),
    })
}

/// The refusal of what a newer client follows, which this hub doesn't know.
pub(super) fn unknown(what: &str) -> ApiError {
    ApiError::new(ErrorCode::Unsupported, format!("{what} this hub doesn't know"))
}
