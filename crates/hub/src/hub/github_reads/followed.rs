//! What clients follow of GitHub, as the hub's list of who follows what says: what the hub sends them as it changes,
//! and what it reads more often than what nobody follows.

use std::sync::Arc;
use std::time::Instant;

use protocol::model::{GithubRepositoryName, IssueNumber, ReferenceAsk, WorkspaceId};

use super::super::Hub;
use super::references::FRESH_FOR;

impl Hub {
    /// Reads again the issues and pull requests clients follow, as chips show them, once what the cache has of each is
    /// [`FRESH_FOR`] old, so one on screen says it's merged within a minute or two of it merging, each time as soon as
    /// it's due. A pull request a read for its workspace keeps the summary of is due no sooner. And, as often as it
    /// tries one again, the pull requests of archived workspaces that the cache has forgotten, as
    /// [`Hub::read_forgotten_followed`] says. Runs until the hub stops.
    #[expect(clippy::disallowed_methods, reason = "the pace what clients follow is looked at")]
    pub(crate) async fn watch_followed_references(self: Arc<Self>) {
        let pace = self.timers.read_pace;
        let mut looked_for_forgotten = Instant::now();
        loop {
            tokio::time::sleep(pace).await;
            let followed = self.followers.references();
            if !followed.is_empty() {
                self.read_summaries(&followed, FRESH_FOR).await;
            }
            if looked_for_forgotten.elapsed() >= self.timers.read_forgotten_again {
                looked_for_forgotten = Instant::now();
                self.read_forgotten_followed().await;
            }
        }
    }

    /// Reads what a subscription has newly followed, as a client asking for it would: the summaries of `references`
    /// the cache has none of within [`FRESH_FOR`], and the pull requests of `workspaces`, each as a client's ask for it
    /// reads it. In the background, so what follows them sends what's kept as it's kept.
    pub(crate) fn read_newly_followed(self: &Arc<Self>, references: Vec<ReferenceAsk>, workspaces: Vec<WorkspaceId>) {
        if !references.is_empty() {
            let hub = self.clone();
            tokio::spawn(async move {
                let asks: Vec<(GithubRepositoryName, IssueNumber)> =
                    references.into_iter().map(|ask| (ask.repository, ask.number)).collect();
                hub.read_summaries(&asks, FRESH_FOR).await;
            });
        }
        for workspace in workspaces {
            let hub = self.clone();
            tokio::spawn(async move {
                if let Err(error) = hub.workspace_pull_request(workspace.clone()).await {
                    tracing::debug!(%workspace, "reading a pull request followed: {error}");
                }
            });
        }
    }
}
