//! What clients show of GitHub, as each of their subscriptions says: what the hub sends them as it changes, and what
//! it reads more often than what nobody shows.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use protocol::client::github::GithubShown;
use protocol::model::{GithubRepositoryName, IssueNumber, ReferenceAsk, ReferenceRead, WorkspaceId};
use tokio::sync::broadcast;

use super::super::Hub;
use super::references::FRESH_FOR;
use crate::github_cache::Item;

/// Which subscription of which connection shows something.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ShownBy {
    pub(crate) connection: u64,
    pub(crate) subscription: u64,
}

/// What every subscription shows, by which it is.
#[derive(Default)]
pub(crate) struct Shown(Mutex<HashMap<ShownBy, GithubShown>>);

impl Shown {
    /// The workspaces whose pull requests some client shows in full.
    pub(crate) fn closely(&self) -> HashSet<WorkspaceId> {
        self.0.lock().values().flat_map(|shown| shown.closely.iter().cloned()).collect()
    }

    /// The workspaces some client shows the pull requests of.
    pub(super) fn workspaces(&self) -> HashSet<WorkspaceId> {
        self.0.lock().values().flat_map(|shown| shown.workspaces.iter().cloned()).collect()
    }

    /// The issues and pull requests some client shows as chips, each once.
    pub(super) fn references(&self) -> Vec<(GithubRepositoryName, IssueNumber)> {
        let mut all: Vec<(GithubRepositoryName, IssueNumber)> = Vec::new();
        for ask in self.0.lock().values().flat_map(|shown| shown.references.iter()) {
            let item = Item::new(&ask.repository, ask.number);
            if !all.iter().any(|(repository, number)| Item::new(repository, *number) == item) {
                all.push((ask.repository.clone(), ask.number));
            }
        }
        all
    }
}

/// Ends what a subscription shows as it's dropped: when the subscription ends, or its connection does.
pub(crate) struct ShowingEnds {
    hub: std::sync::Weak<Hub>,
    by: ShownBy,
}

impl ShowingEnds {
    /// Takes what it ends to show `shown` from now on, in place of what it showed.
    pub(crate) fn show(&self, shown: GithubShown) {
        if let Some(hub) = self.hub.upgrade() {
            hub.github_shown.0.lock().insert(self.by, shown);
        }
    }
}

impl Drop for ShowingEnds {
    fn drop(&mut self) {
        if let Some(hub) = self.hub.upgrade() {
            hub.github_shown.0.lock().remove(&self.by);
        }
    }
}

impl Hub {
    /// Whether any subscription shows anything of GitHub, for a test of what ends what's shown.
    #[cfg(test)]
    pub(crate) fn shows_github(&self) -> bool {
        !self.github_shown.0.lock().is_empty()
    }

    /// Reads again what clients show as chips once what the cache has of it is [`FRESH_FOR`] old, so one on screen says
    /// it's merged within a minute or two of it merging, each time as soon as it's due. A pull request a read for its
    /// workspace keeps the summary of is due no sooner. And, as often as it tries one again, the pull requests of
    /// archived workspaces clients show that the cache has forgotten, as [`Hub::read_forgotten`] says. Runs until the
    /// hub stops.
    #[expect(clippy::disallowed_methods, reason = "the pace what clients show is looked at")]
    pub(crate) async fn watch_shown_references(self: Arc<Self>) {
        let pace = self.timers.read_pace;
        let mut looked_for_forgotten = Instant::now();
        loop {
            tokio::time::sleep(pace).await;
            let shown = self.github_shown.references();
            if !shown.is_empty() {
                self.read_summaries(&shown, FRESH_FOR).await;
            }
            if looked_for_forgotten.elapsed() >= self.timers.read_forgotten_again {
                looked_for_forgotten = Instant::now();
                self.read_forgotten_shown().await;
            }
        }
    }

    /// Takes `shown` to be what `by` shows from now on, in place of what it showed, and returns what ends it.
    pub(crate) fn show_github(self: &Arc<Self>, by: ShownBy, shown: GithubShown) -> ShowingEnds {
        self.github_shown.0.lock().insert(by, shown);
        ShowingEnds { hub: Arc::downgrade(self), by }
    }

    /// Tells of each read the cache keeps from now on, by the issues and pull requests it was of, for a client's
    /// subscription to send what changed.
    pub(crate) fn github_kept(&self) -> broadcast::Receiver<Vec<Item>> {
        self.github_cache.changes()
    }

    /// What the cache has of an issue or a pull request a chip shows, however old.
    pub(crate) fn reference_kept(&self, ask: &ReferenceAsk) -> Option<ReferenceRead> {
        self.github_cache.summary(&Item::new(&ask.repository, ask.number))
    }

    /// Reads what's newly shown, as a client asking for it would: the summaries of `references` the cache has none of
    /// within [`FRESH_FOR`], and the pull requests of `closely`, each as a client's ask for it reads it. In the
    /// background, so what shows them sends what's kept as it's kept.
    pub(crate) fn read_newly_shown(self: &Arc<Self>, references: Vec<ReferenceAsk>, closely: Vec<WorkspaceId>) {
        if !references.is_empty() {
            let hub = self.clone();
            tokio::spawn(async move {
                let asks: Vec<(GithubRepositoryName, IssueNumber)> =
                    references.into_iter().map(|ask| (ask.repository, ask.number)).collect();
                hub.read_summaries(&asks, FRESH_FOR).await;
            });
        }
        for workspace in closely {
            let hub = self.clone();
            tokio::spawn(async move {
                if let Err(error) = hub.workspace_pull_request(workspace.clone()).await {
                    tracing::debug!(%workspace, "reading a pull request shown: {error}");
                }
            });
        }
    }
}
