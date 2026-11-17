//! What a repository's workers are told to make its worktrees from.

use protocol::model::Repo;
use protocol::worker::RepoSource;

/// The repository's checkout on the hub's machine, which every repository has, since it's added by its checkout.
/// *Not built:* a repository added by its address alone, whose workers clone it.
pub(super) fn repo_source(repo: &Repo) -> RepoSource {
    RepoSource::Checkout { path: repo.path.clone() }
}
