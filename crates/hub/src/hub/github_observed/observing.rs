//! The task that works out each observation of GitHub, and what tells it to.

use std::sync::Arc;

use protocol::client::{ApiError, HubNotice};
use protocol::model::RecordKey;
use tokio::sync::broadcast::error::RecvError;
use tracing::warn;

use super::chips::Chips;
use super::workspaces::Workspaces;
use crate::hub::Hub;

/// What the task knows of what it works out.
#[derive(Default)]
struct Observing {
    chips: Chips,
    workspaces: Workspaces,
}

impl Hub {
    /// Works out each observation of GitHub whenever what it's worked out from changes, as `super` says, starting with
    /// removing those of chips whose issue or pull request the cache no longer has, as after it forgot what was kept
    /// for too long while the hub was away, and working out every workspace's. Runs until the hub stops.
    pub(crate) async fn observe_github(self: Arc<Self>) {
        // Before anything is worked out, so nothing that changes after is missed.
        let mut told = self.github_cache.changes();
        let mut followers = self.followers.changes();
        let mut feed = self.subscribe_feed();
        let mut unseen = self.unseen_told();
        let mut reads = self.github_reads_ended.subscribe();
        let mut observing = Observing::default();
        observing.all_again(&self).await;
        loop {
            tokio::select! {
                changed = followers.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    observing.follow_now(&self).await;
                }
                told_now = told.recv() => match told_now {
                    Ok(told_now) => {
                        // First, so a read kept just after something began to follow it is current, whichever the task
                        // hears of first.
                        observing.follow_now(&self).await;
                        observing.chips.told(&self, &told_now).await;
                        observing.workspaces.told(&self, &told_now).await;
                    }
                    Err(RecvError::Lagged(_)) => observing.all_again(&self).await,
                    Err(RecvError::Closed) => return,
                },
                notice = feed.recv() => match notice.as_deref() {
                    Ok(HubNotice::Workspace(workspace)) => observing.workspaces.recorded(&self, workspace).await,
                    Ok(HubNotice::Removed { record: RecordKey::Workspace { id }, .. }) => {
                        observing.workspaces.removed(&self, id).await;
                    }
                    Ok(_) => {}
                    Err(RecvError::Lagged(_)) => observing.all_again(&self).await,
                    Err(RecvError::Closed) => return,
                },
                id = unseen.recv() => match id {
                    Ok(id) => observing.workspaces.unseen(&self, &id).await,
                    Err(RecvError::Lagged(_)) => observing.all_again(&self).await,
                    Err(RecvError::Closed) => return,
                },
                ended = reads.recv() => match ended {
                    Ok(ended) => {
                        observing.follow_now(&self).await;
                        observing.workspaces.read_ended(&self, &ended.workspace, ended.failed).await;
                    }
                    Err(RecvError::Lagged(_)) => observing.all_again(&self).await,
                    Err(RecvError::Closed) => return,
                },
            }
        }
    }
}

impl Hub {
    /// Works out every observation of GitHub once, as the task does as it starts, for a test of what that sends.
    #[cfg(test)]
    pub(crate) async fn observe_github_once(&self) {
        Observing::default().all_again(self).await;
    }
}

impl Observing {
    /// Takes in what clients follow now.
    async fn follow_now(&mut self, hub: &Hub) {
        self.chips.follow_now(hub).await;
        self.workspaces.follow_now(hub).await;
    }

    /// Works out everything again, as as the hub starts, or after missing some of what the task is told.
    async fn all_again(&mut self, hub: &Hub) {
        hub.forget_unkept_chips().await;
        self.follow_now(hub).await;
        self.chips.work_out_all(hub).await;
        self.workspaces.all_again(hub).await;
    }
}

/// Says in the log why a change to an observation failed, which only a database that gives no revision fails, so it's
/// as it was until what it's worked out from changes again.
pub(super) fn logged(changed: Result<(), ApiError>) {
    if let Err(error) = changed {
        warn!("keeping an observation of GitHub: {}", error.message);
    }
}
