//! What a repository's workers are told to make its worktrees from.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::Repo;
use protocol::worker::RepoSource;

/// The repository's checkout on the hub's machine, which what makes and keeps a workspace's worktrees needs, so one
/// with none is unavailable here. *Not built:* a repository added by its address alone, whose workers clone it, which
/// the project bullet of "State ownership" in `docs/architecture.md` plans, so every repository the hub keeps has a
/// checkout.
pub(super) fn repo_source(repo: &Repo) -> Result<RepoSource, ApiError> {
    let path = repo.origin.checkout().ok_or_else(|| {
        ApiError::new(ErrorCode::Unavailable, format!("{} has no checkout on the hub's machine", repo.name))
    })?;
    Ok(RepoSource::Checkout { path: path.to_owned() })
}
