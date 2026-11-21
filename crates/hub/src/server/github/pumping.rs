//! Feeding a subscription to what GitHub says of what a client shows.

use std::collections::HashMap;
use std::sync::Arc;

use protocol::client::github::{GithubShown, GithubUpdate};
use protocol::client::{HubFrame, HubNotice};
use protocol::model::{PullRequestFound, ReferenceAsk, ReferenceRead, Workspace, WorkspaceId};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{mpsc, watch};
use tracing::warn;

use crate::github_cache::Item;
use crate::hub::Hub;
use crate::server::Outgoing;

/// Feeds one subscription: what the hub knows of each of what's shown, then what changes of it, from the reads the
/// cache keeps and the workspaces' records, until the subscription or its connection ends.
pub(super) async fn pump(
    hub: Arc<Hub>,
    mut shown: watch::Receiver<GithubShown>,
    subscription_id: u64,
    frames: mpsc::Sender<Outgoing>,
) {
    // Before anything's read to send, so nothing kept meanwhile is missed.
    let mut kept = hub.github_kept();
    let mut feed = hub.subscribe_feed();
    let mut feeding = Feeding { hub, shown: GithubShown::default(), sent: Sent::default() };
    let deliver = async |updates: Vec<GithubUpdate>| {
        for update in updates {
            frames.send(HubFrame::Github { subscription_id, update }.into()).await.map_err(|_| ())?;
        }
        Ok::<(), ()>(())
    };
    'whole: loop {
        let now = shown.borrow_and_update().clone();
        if deliver(feeding.show(now).await).await.is_err() {
            return;
        }
        loop {
            let updates = tokio::select! {
                changed = shown.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    continue 'whole;
                }
                items = kept.recv() => match items {
                    Ok(items) => feeding.kept(&items),
                    Err(RecvError::Lagged(_)) => continue 'whole,
                    Err(RecvError::Closed) => return,
                },
                notice = feed.recv() => match notice {
                    Ok(HubNotice::Workspace(workspace)) => feeding.recorded(&workspace),
                    Ok(_) => Vec::new(),
                    Err(RecvError::Lagged(_)) => continue 'whole,
                    Err(RecvError::Closed) => return,
                },
            };
            if deliver(updates).await.is_err() {
                return;
            }
        }
    }
}

/// What one subscription shows, and has sent of it.
struct Feeding {
    hub: Arc<Hub>,
    shown: GithubShown,
    sent: Sent,
}

/// What a subscription has sent of each of what it shows, so it sends only what changed.
#[derive(Default)]
struct Sent {
    references: HashMap<ReferenceAsk, ReferenceRead>,
    /// `None` where it said the hub hasn't looked.
    pull_requests: HashMap<WorkspaceId, Option<PullRequestFound>>,
}

impl Feeding {
    /// Takes `now` to be what's shown, reads what's newly shown, and gives what's to send of all of it, which is what
    /// wasn't sent last, so a client that shows something again, or has missed some, is sent what it needs.
    async fn show(&mut self, now: GithubShown) -> Vec<GithubUpdate> {
        let before = std::mem::replace(&mut self.shown, now);
        let newly = (self.shown.references.iter()).filter(|ask| !before.references.contains(ask)).cloned().collect();
        let closely = (self.shown.closely.iter()).filter(|id| !before.closely.contains(id)).cloned().collect();
        self.hub.read_newly_shown(newly, closely);
        let shown = &self.shown;
        self.sent.references.retain(|ask, _| shown.references.contains(ask));
        self.sent.pull_requests.retain(|id, _| shown.workspaces.contains(id));
        let mut updates = Vec::new();
        for ask in &self.shown.references {
            updates.extend(self.sent.reference(ask, self.hub.reference_kept(ask)));
        }
        let workspaces = self.hub.workspace_list(true).await.unwrap_or_else(|error| {
            warn!("listing the workspaces whose pull requests a client shows: {error}");
            Vec::new()
        });
        for workspace in workspaces.iter().filter(|workspace| self.shown.workspaces.contains(&workspace.id)) {
            updates.extend(self.sent.pull_request(workspace));
        }
        updates
    }

    /// What's to send now that the cache has kept a read of `items`.
    fn kept(&mut self, items: &[Item]) -> Vec<GithubUpdate> {
        let told = self.shown.references.iter().filter(|ask| items.iter().any(|item| item.is(ask)));
        told.filter_map(|ask| self.sent.reference(ask, self.hub.reference_kept(ask))).collect()
    }

    /// What's to send now that `workspace`'s record has changed.
    fn recorded(&mut self, workspace: &Workspace) -> Vec<GithubUpdate> {
        if !self.shown.workspaces.contains(&workspace.id) {
            return Vec::new();
        }
        self.sent.pull_request(workspace).into_iter().collect()
    }
}

impl Sent {
    /// What to send of `ask`, now that the cache has `kept` of it, unless that's what was sent last.
    fn reference(&mut self, ask: &ReferenceAsk, kept: Option<ReferenceRead>) -> Option<GithubUpdate> {
        let read = kept?;
        if self.references.get(ask) == Some(&read) {
            return None;
        }
        self.references.insert(ask.clone(), read.clone());
        Some(GithubUpdate::Reference { reference: ask.clone(), read })
    }

    /// What to send of `workspace`'s pull request, as its record has it, unless that's what was sent last. Even that
    /// the hub hasn't looked is sent the first time, so a client that heard otherwise on a connection before hears it.
    fn pull_request(&mut self, workspace: &Workspace) -> Option<GithubUpdate> {
        let found = workspace.pull_request.clone();
        if self.pull_requests.get(&workspace.id) == Some(&found) {
            return None;
        }
        self.pull_requests.insert(workspace.id.clone(), found.clone());
        Some(GithubUpdate::PullRequest { workspace: workspace.id.clone(), found })
    }
}
