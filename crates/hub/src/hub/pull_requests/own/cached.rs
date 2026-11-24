//! A workspace's own pull request as the hub knows it, from the link its record keeps and what the cache has of
//! GitHub's word on it, and as the cache has it for a client that asks just after another read, so the hub doesn't
//! read GitHub again for it.

use std::time::Duration;

use protocol::model::{
    Local, OwnPullRequest, PrState, PullRequest, PullRequestFound, PullRequestLink, Workspace, WorkspaceId,
};

use super::judging::local;
use crate::github_cache::Item;
use crate::hub::Hub;
use crate::hub::github_reads::RULES_FRESH_FOR;
use crate::hub::observations::WorkspacePullRequest;
use crate::hub::pull_requests::recording::Found;

impl Hub {
    /// The workspace's own pull request as the hub knows it: which it is, as its record keeps it, and what GitHub last
    /// said of it, in whichever facets the cache has however old, its checks judged by its branch's rules where the
    /// cache has them, and how the worktree stands against it, as the hub knows its branches, or as it was last worked
    /// out while they aren't known, with as many files uncommitted as [`Hub::uncommitted_known`] says. `None` until the hub has looked, and while the cache has nothing
    /// of the one it found, as after the cache was emptied, until it's read again.
    pub(crate) fn known_of(&self, workspace: &Workspace) -> Option<PullRequestFound> {
        let linked = workspace.pull_request_link.as_ref()?;
        let Some(link) = &linked.link else { return Some(PullRequestFound { pull_request: None }) };
        let item = Item::new(&link.repository, link.number);
        let pull_request =
            self.github_cache.pull_request(&item).filter(|pull_request| is_linked(pull_request, link))?;
        let uncommitted = self.uncommitted_known(&workspace.id, &link.url).unwrap_or_default();
        let local = match self.branches_of(&workspace.id) {
            Some(branches) => match branches.checked_out() {
                Some(branch) => local(&pull_request.said().head, branch, uncommitted),
                None => Local { uncommitted, ..Local::default() },
            },
            // Not known, so it's as it was last worked out, from its last read, rather than up to date with GitHub.
            None => Local { uncommitted, ..self.worked_out(&workspace.id, &link.url).unwrap_or_default() },
        };
        Some(PullRequestFound { pull_request: Some(OwnPullRequest { pull_request, local }) })
    }

    /// How many files were uncommitted in workspace `id`'s worktree as its pull request at `url` was last read: as that
    /// read counted them since the hub started, or else as the observation of the workspace's pull request says, which
    /// the cache file keeps. `None` where neither is of that pull request.
    pub(crate) fn uncommitted_known(&self, id: &WorkspaceId, url: &str) -> Option<u32> {
        if let Some(counted) = self.pull_requests.counted.lock().get(id).filter(|counted| counted.url == url) {
            return Some(counted.files);
        }
        self.worked_out(id, url).map(|local| local.uncommitted)
    }

    /// How the worktree of workspace `id` stood against its pull request at `url` as the observation of that pull
    /// request last worked it out, or `None` where it's of none, or of another.
    fn worked_out(&self, id: &WorkspaceId, url: &str) -> Option<Local> {
        let observed = self.observations.value_of::<WorkspacePullRequest>(id)?.pull_request?;
        (observed.pull_request.summary.about.url == url).then_some(observed.local)
    }

    /// The workspace's own pull request, as the cache has it from a read that began within
    /// [`crate::Timers::fresh_for_clients`], with its branch's rules for its checks, and how the worktree stands
    /// against it now. Only an open one that was found on the branch checked out, since which pull request is the
    /// workspace's can't have changed while that's open, but a branch with none can have one opened at any time. And
    /// only where its head was where the branch or its upstream is, so a push since the read was read again for.
    pub(in crate::hub::pull_requests) async fn own_from_cache(&self, workspace: &Workspace) -> Option<Found> {
        let link = workspace.pull_request_linked()?;
        let branches = self.branches_of(&workspace.id)?;
        let branch = branches.checked_out()?;
        if workspace.pull_request_branch.as_ref() != Some(&branch.name) {
            return None;
        }
        let pull_request = self.observed(link, self.timers.fresh_for_clients)?;
        let head = &pull_request.said().head;
        let upstream = branch.upstream.as_ref().and_then(|upstream| upstream.tip.as_ref());
        let at_head = *head == branch.tip || upstream == Some(head);
        if pull_request.said().state != PrState::Open || !at_head {
            return None;
        }
        let known = || self.uncommitted_known(&workspace.id, &link.url).unwrap_or_default();
        let files = self.uncommitted_in(workspace).await.unwrap_or_else(known);
        let local = local(head, branch, files);
        Some(Found::of(OwnPullRequest { pull_request, local }, link.repository.clone(), Some(branch.name.clone())))
    }

    /// `link`'s pull request as the cache has it, where each of its facets was read within `fresh_for`, with its checks
    /// judged by its branch's rules, read within [`RULES_FRESH_FOR`]. `None` where the cache has none, or what it has is
    /// of another pull request of that number, as a fork's is.
    fn observed(&self, link: &PullRequestLink, fresh_for: Duration) -> Option<PullRequest> {
        let item = Item::new(&link.repository, link.number);
        (self.github_cache.fresh_pull_request(&item, fresh_for, RULES_FRESH_FOR))
            .filter(|pull_request| is_linked(pull_request, link))
    }
}

/// Whether `pull_request` is the one `link` names, rather than another of that number, as a fork's is.
fn is_linked(pull_request: &PullRequest, link: &PullRequestLink) -> bool {
    pull_request.said().about.url == link.url
}
