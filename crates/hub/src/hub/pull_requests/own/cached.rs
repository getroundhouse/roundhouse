//! A workspace's own pull request as the hub knows it, from the link its record keeps and what the cache has of
//! GitHub's word on it, and as the cache has it for a client that asks just after another read, so the hub doesn't
//! read GitHub again for it.

use std::time::Duration;

use github::Rules;
use protocol::model::{Local, ObservedPullRequest, PrState, PullRequestFound, PullRequestLink, Workspace};

use super::judging::local;
use super::reading::checked_out;
use crate::github_cache::Item;
use crate::hub::Hub;
use crate::hub::github_reads::RULES_FRESH_FOR;
use crate::hub::pull_requests::reading::Found;

impl Hub {
    /// The workspace's own pull request as the hub knows it: which it is, as its record keeps it, and what GitHub last
    /// said of it, as the cache has it however old, with its branch's rules for its checks, and how the worktree stands
    /// against it, as the record has its branches. `None` until the hub has looked, and while the cache has nothing of
    /// the one it found, as after the cache was emptied, until it's read again.
    pub(crate) fn known_of(&self, workspace: &Workspace) -> Option<PullRequestFound> {
        let linked = workspace.pull_request_link.as_ref()?;
        let Some(link) = &linked.link else { return Some(PullRequestFound { pull_request: None }) };
        let (observed, _) = self.observed(link, Duration::MAX, Duration::MAX)?;
        let mut pull_request = observed.pull_request;
        pull_request.local = match checked_out(workspace) {
            Some(branch) => local(&observed.head, branch, link.uncommitted),
            None => Local { uncommitted: link.uncommitted, ..Local::default() },
        };
        Some(PullRequestFound { pull_request: Some(pull_request) })
    }

    /// The workspace's own pull request, as the cache has it from a read that began within
    /// [`crate::Timers::fresh_for_clients`], with its branch's rules for its checks, and how the worktree stands
    /// against it now. Only an open one that was found on the branch checked out, since which pull request is the
    /// workspace's can't have changed while that's open, but a branch with none can have one opened at any time. And
    /// only where its head was where the branch or its upstream is, so a push since the read was read again for.
    pub(in crate::hub::pull_requests) async fn own_from_cache(&self, workspace: &Workspace) -> Option<Found> {
        let link = workspace.pull_request_linked()?;
        let branch = checked_out(workspace)?;
        if workspace.pull_request_branch.as_ref() != Some(&branch.name) {
            return None;
        }
        let (observed, judged) = self.observed(link, self.timers.fresh_for_clients, RULES_FRESH_FOR)?;
        let upstream = branch.upstream.as_ref().and_then(|upstream| upstream.tip.as_ref());
        let at_head = observed.head == branch.tip || upstream == Some(&observed.head);
        if observed.pull_request.state != PrState::Open || !judged || !at_head {
            return None;
        }
        let files = self.uncommitted_in(workspace).await.unwrap_or(link.uncommitted);
        let mut pull_request = observed.pull_request;
        pull_request.local = local(&observed.head, branch, files);
        Some(Found::of(pull_request, link.repository.clone(), Some(branch.name.clone())))
    }

    /// `link`'s pull request as the cache has it, where each of its facets was read within `fresh_for`, put together
    /// with its branch's rules for its checks where they were read within `rules_fresh_for`, and whether its checks could
    /// be judged by them.
    /// `None` where the cache has none, or what it has is of another pull request of that number, as a fork's is.
    fn observed(
        &self,
        link: &PullRequestLink,
        fresh_for: Duration,
        rules_fresh_for: Duration,
    ) -> Option<(ObservedPullRequest, bool)> {
        let item = Item::new(&link.repository, link.number);
        let facets = self.github_cache.pull(&item, fresh_for).filter(|facets| facets.status.url == link.url)?;
        let rules: Option<Rules> = (self.github_cache.rules_of(&link.repository, rules_fresh_for).into_iter())
            .find(|(branch, _)| *branch == facets.stack.destination)
            .map(|(_, rules)| rules);
        // A merged or closed one has no checks to judge, so it needs none.
        let judged = rules.is_some() || facets.status.state != PrState::Open;
        Some((github::observe(facets, rules.as_ref()), judged))
    }
}
