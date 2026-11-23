//! A subscription at a workspace's level: every observation of the workspace, then each notice of its level, and every
//! observation again whenever it has fallen behind. Following a workspace is also what has the hub read what GitHub
//! says of its pull request closely, as the hub's list of followers says.

use std::sync::Arc;

use protocol::client::ApiError;
use protocol::client::listed::Listed;
use protocol::model::{Level, WorkspaceId};
use tokio::sync::mpsc;

use super::notices::{forward, send_listed};
use super::starting::Feeding;
use crate::hub::Hub;
use crate::server::Outgoing;

/// What feeds a subscription at workspace `id`'s level, once its first list is queued, so one the hub doesn't have is
/// refused. The list is queued here, before the reply to the `Subscribe` that starts it, rather than by what feeds it,
/// so a client that has the reply with no list before it knows the hub is from before workspaces had a level.
pub(super) async fn feeding(
    hub: &Arc<Hub>,
    id: WorkspaceId,
    subscription_id: u64,
    frames: mpsc::Sender<Outgoing>,
) -> Result<Feeding, ApiError> {
    hub.workspace(&id).await?;
    // Before the list, so nothing that changes after it is missed.
    let feed = hub.subscribe_feed();
    let listed = hub.workspace_observations(&id).await?;
    let connected = send_listed(&frames, subscription_id, Listed::Workspace(listed)).await;
    let hub = hub.clone();
    Ok(Box::pin(async move {
        if !connected {
            return;
        }
        let level = Level::Workspace { id: id.clone() };
        let list = || async { Ok(Listed::Workspace(hub.workspace_observations(&id).await?)) };
        forward(feed, &level, subscription_id, &frames, list, |_| {}).await;
    }))
}
