//! A workspace's own pull request as the cache has it, for a client that asks just after another read, so the hub
//! doesn't read GitHub again for it.

use protocol::model::{IssueNumber, PrState, PullRequestFound, RepoUrl, Workspace};

use super::judging::local;
use super::reading::checked_out;
use crate::github_cache::Item;
use crate::hub::Hub;
use crate::hub::pull_requests::reading::Found;

impl Hub {
    /// The workspace's own pull request, as the cache has it from a read that began within
    /// [`crate::Timers::fresh_for_clients`], with its branch's rules for its checks, and how the worktree stands
    /// against it now. Only an open one that was found on the branch checked out, since which pull request is the
    /// workspace's can't have changed while that's open, but a branch with none can have one opened at any time. And
    /// only where its head was where the branch or its upstream is, so a push since the read was read again for.
    pub(in crate::hub::pull_requests) async fn own_from_cache(&self, workspace: &Workspace) -> Option<Found> {
        let known = workspace.known_pull_request().filter(|known| known.state == PrState::Open)?;
        let branch = checked_out(workspace)?;
        if workspace.pull_request_branch.as_ref() != Some(&branch.name) {
            return None;
        }
        let repo = self.repo_of(workspace).await.ok()?;
        let into = repo.origin.url().and_then(RepoUrl::on_github)?;
        let item = Item::new(&into, IssueNumber::try_from(known.number).ok()?);
        // The repository its work merges into has a pull request of that number, but a fork's merges into another.
        let facets = self
            .github_cache
            .pull(&item, self.timers.fresh_for_clients)
            .filter(|facets| facets.status.url == known.url)
            .filter(|facets| {
                let upstream = branch.upstream.as_ref().and_then(|upstream| upstream.tip.as_ref());
                facets.status.head == branch.tip || upstream == Some(&facets.status.head)
            })?;
        let rules = self.github_cache.rules_of(&into, crate::hub::github_reads::RULES_FRESH_FOR);
        let rules = rules.into_iter().find(|(branch, _)| *branch == facets.stack.destination).map(|(_, rules)| rules);
        if facets.status.state == PrState::Open && rules.is_none() {
            return None;
        }
        let observed = github::observe(facets, rules.as_ref());
        let files = self.uncommitted_in(workspace).await.unwrap_or(known.local.uncommitted);
        let mut pull_request = observed.pull_request;
        pull_request.local = local(&observed.head, branch, files);
        Some(Found { found: PullRequestFound { pull_request: Some(pull_request) }, on: Some(branch.name.clone()) })
    }
}
