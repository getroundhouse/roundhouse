//! Reading again the pull requests of archived workspaces clients show, once the cache has forgotten them, as after it
//! was emptied, since nothing reads an archived workspace's in turn. By the number each one's record links to, and into
//! the cache alone, which tells what shows them, recording nothing, so the record keeps its link whatever's read.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use github::{Leaving, PullRequestsAsk};
use parking_lot::Mutex;
use protocol::client::ApiError;
use protocol::model::{PullRequestLink, PullRequestNumber, PullRequestSelection, Workspace, WorkspaceStatus};

use super::super::Hub;
use crate::github_cache::Item;

/// When the hub last tried to read each of them, and how long it waits from then before it tries again, so one that
/// couldn't be read, or that GitHub no longer has, is tried less often each time, however many clients show it.
#[derive(Default)]
pub(crate) struct Forgotten(Mutex<HashMap<Item, Tried>>);

#[derive(Clone, Copy)]
struct Tried {
    at: Instant,
    wait: Duration,
}

/// How many times [`crate::Timers::read_forgotten_again`] the longest wait between tries is: half an hour, as the
/// longest between reads in turn is.
const LONGEST: u32 = 30;

impl Forgotten {
    /// Those of `links` that are due, which are taken to be tried at `now`: each the first time, then `again` after
    /// that, and twice as long after each try, up to [`LONGEST`] times as long. One not tried for twice that is
    /// forgotten, as one that has been read is.
    pub(super) fn due(&self, links: Vec<PullRequestLink>, again: Duration, now: Instant) -> Vec<PullRequestLink> {
        let longest = again.saturating_mul(LONGEST);
        let mut tried = self.0.lock();
        tried.retain(|_, tried| now.duration_since(tried.at) < longest.saturating_mul(2));
        (links.into_iter())
            .filter(|link| {
                let item = Item::new(&link.repository, link.number);
                let wait = match tried.get(&item) {
                    None => again,
                    Some(last) if now.duration_since(last.at) >= last.wait => last.wait.saturating_mul(2).min(longest),
                    Some(_) => return false,
                };
                tried.insert(item, Tried { at: now, wait });
                true
            })
            .collect()
    }
}

impl Hub {
    /// The link to `workspace`'s pull request where only this reads it: where it's archived, and the cache is missing
    /// any of its facets, as it is of one only a chip's read has found, which has only its summary.
    pub(crate) fn forgotten_link(&self, workspace: &Workspace) -> Option<PullRequestLink> {
        if workspace.status != WorkspaceStatus::Archived {
            return None;
        }
        let known = self.known_of(workspace).and_then(|found| found.pull_request);
        let whole = known.is_some_and(|own| {
            let pull_request = own.pull_request;
            pull_request.history.is_some() && pull_request.stack.is_some() && pull_request.checks.is_some()
        });
        workspace.pull_request_linked().filter(|_| !whole).cloned()
    }

    /// Reads each pull request of `links` that's due, as [`Forgotten`] says, as a read nobody waits on, in the
    /// background.
    pub(crate) fn read_forgotten(self: &Arc<Self>, links: Vec<PullRequestLink>) {
        let links = self.github_forgotten.due(links, self.timers.read_forgotten_again, Instant::now());
        if links.is_empty() {
            return;
        }
        let hub = self.clone();
        tokio::spawn(async move {
            if let Err(error) = hub.read_forgotten_now(&links).await {
                tracing::debug!("reading pull requests the cache had forgotten: {}", error.message);
            }
        });
    }

    /// Reads those of the archived workspaces clients show, as [`Hub::read_forgotten`] does.
    pub(super) async fn read_forgotten_shown(self: &Arc<Self>) {
        let shown = self.github_shown.workspaces();
        if shown.is_empty() {
            return;
        }
        let Ok(workspaces) = self.workspace_list(true).await else { return };
        let links = (workspaces.iter())
            .filter(|workspace| shown.contains(&workspace.id))
            .filter_map(|workspace| self.forgotten_link(workspace))
            .collect();
        self.read_forgotten(links);
    }

    async fn read_forgotten_now(&self, links: &[PullRequestLink]) -> Result<(), ApiError> {
        let (reads_as, token) = self.reading_as()?;
        self.may_read_unasked(&reads_as)?;
        let asks = (links.iter())
            .filter_map(|link| {
                let number = PullRequestNumber::try_from(link.number.get()).ok()?;
                let selection = PullRequestSelection::from(number);
                Some(PullRequestsAsk { repository: link.repository.clone(), selection, known_rules: Vec::new() })
            })
            .collect();
        let read = self.read_pull_requests(&token, asks, Leaving::ForOthers).await;
        self.github_said(&reads_as, read.rate_limit);
        Ok(())
    }
}
