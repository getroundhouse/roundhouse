//! A subscription at the hub's level: where every record but a session's own stands, then each notice of the hub's
//! level, and what GitHub says of each workspace's own pull request: as an observation to one that reads GitHub so, and
//! otherwise as a `GithubSubscribe` that shows every workspace is sent it.

use std::sync::Arc;

use protocol::client::listed::Listed;
use protocol::client::{ApiError, HubNotice};
use protocol::model::{RecordKey, Records};
use tokio::sync::{mpsc, watch};

use super::notices::{forward, send_listed};
use super::starting::Feeding;
use crate::hub::Hub;
use crate::server::github::{GithubShown, pump};
use crate::server::sends::Sends;
use crate::server::{Closer, Outgoing};

/// What feeds a subscription at the hub's level, once the hub has listed its records, so one it can't list is refused.
pub(super) async fn feeding(
    hub: &Arc<Hub>,
    sends: Sends,
    subscription_id: u64,
    frames: mpsc::Sender<Outgoing>,
    closer: Closer,
) -> Result<Feeding, ApiError> {
    // Before the list, so nothing that changes after it is missed.
    let feed = hub.subscribe_fed();
    let listed = hub.records_list().await?;
    let hub = hub.clone();
    Ok(Box::pin(async move {
        let (shown, showing) = watch::channel(every_workspace(&listed));
        if !send_listed(&frames, &sends, subscription_id, Listed::Hub(Box::new(listed))).await {
            return;
        }
        let github = async {
            if sends.github_observed() {
                return std::future::pending().await;
            }
            pump(hub.clone(), showing, subscription_id, frames.clone()).await;
        };
        let list = || async {
            let listed = hub.records_list().await?;
            shown.send_replace(every_workspace(&listed));
            Ok(Listed::Hub(Box::new(listed)))
        };
        let seen = |notice: &HubNotice| shows_every_workspace(&shown, notice);
        let records = forward(feed, &sends, subscription_id, &frames, &closer, list, seen);
        // Each ends only once the connection has gone, and with it the other.
        tokio::select! {
            () = github => {}
            () = records => {}
        }
    }))
}

/// What GitHub says that a subscription at the hub's level is sent: every workspace's own pull request.
fn every_workspace(listed: &Records) -> GithubShown {
    let workspaces = listed.workspaces.iter().map(|workspace| workspace.id.clone()).collect();
    GithubShown { workspaces, ..GithubShown::default() }
}

/// Takes in what `notice` says of which workspaces there are, so the pull request of one made since the list is sent,
/// and one that's gone is no longer.
fn shows_every_workspace(shown: &watch::Sender<GithubShown>, notice: &HubNotice) {
    let (id, gone) = if let HubNotice::Workspace(workspace) = notice {
        (&workspace.id, false)
    } else if let HubNotice::Removed { record: RecordKey::Workspace { id }, .. } = notice {
        (id, true)
    } else {
        return;
    };
    shown.send_if_modified(|shown| {
        if shown.workspaces.contains(id) != gone {
            return false;
        }
        if gone {
            shown.workspaces.retain(|shown| shown != id);
        } else {
            shown.workspaces.push(id.clone());
        }
        true
    });
}
