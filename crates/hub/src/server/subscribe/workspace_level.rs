//! A subscription at a workspace's level: every observation of the workspace, then each notice of its level, and every
//! observation again whenever it has fallen behind. Following a workspace is also what has the hub read what GitHub
//! says of its pull request closely, as the hub's list of followers says.

use std::sync::Arc;

use protocol::client::ApiError;
use protocol::client::listed::{Listed, WorkspaceObservations};
use protocol::model::{Level, WorkspaceId};
use tokio::sync::{mpsc, oneshot};

use super::notices::{forward, send_listed};
use super::starting::Feeding;
use crate::hub::Hub;
use crate::server::sends::Sends;
use crate::server::{Closer, Outgoing};

/// What feeds a subscription at workspace `id`'s level, and what says once its first list is queued, so one the hub
/// doesn't have is refused. What feeds it queues the list first, so it goes only to a subscription that has started,
/// and the `Subscribe` that starts it waits for that before its reply, so a client that has the reply with no list
/// before it knows the hub is from before workspaces had a level.
pub(super) async fn feeding(
    hub: &Arc<Hub>,
    id: WorkspaceId,
    sends: Sends,
    subscription_id: u64,
    frames: mpsc::Sender<Outgoing>,
    closer: Closer,
) -> Result<(Feeding, oneshot::Receiver<()>), ApiError> {
    hub.workspace(&id).await?;
    // Before the list, so nothing that changes after it is missed.
    let feed = hub.subscribe_feed();
    let first = listed(hub, &id).await?;
    let (queued, is_queued) = oneshot::channel();
    let hub = hub.clone();
    let feeding: Feeding = Box::pin(async move {
        let connected = send_listed(&frames, &sends, subscription_id, Listed::Workspace(first)).await;
        // An error only means nothing waits for it any more.
        let _ = queued.send(());
        if !connected {
            return;
        }
        let list = || async { Ok(Listed::Workspace(listed(&hub, &id).await?)) };
        forward(feed, &sends, subscription_id, &frames, &closer, list, |_| {}).await;
    });
    Ok((feeding, is_queued))
}

/// Every observation of workspace `id`'s level, as its subscription is listed them.
async fn listed(hub: &Hub, id: &WorkspaceId) -> Result<WorkspaceObservations, ApiError> {
    let (observations, rev) = hub.observations_at(&Level::Workspace { id: id.clone() }).await?;
    Ok(WorkspaceObservations { workspace_id: id.clone(), observations, rev })
}
