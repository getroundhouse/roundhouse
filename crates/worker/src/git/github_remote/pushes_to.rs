use process::Environment;
use std::path::Path;

use protocol::client::ApiError;
use protocol::model::GithubRepositoryName;

use super::address::on_github;
use crate::git::{git, git_error};

/// What `git remote` answers for a remote that isn't there.
const NO_SUCH_REMOTE: i32 = 2;

/// The repository on github.com that `origin` pushes to in the repository at `repo`, as git gives its push address with
/// what the worker gives every process, `given`, or `None` when there's no `origin` or it pushes anywhere else.
pub async fn github_remote(given: &Environment, repo: &Path) -> Result<Option<GithubRepositoryName>, ApiError> {
    let found = git(given, repo, &["remote", "get-url", "--push", "origin"]).await?;
    if found.success {
        return Ok(on_github(found.stdout.trim()));
    }
    if found.code == Some(NO_SUCH_REMOTE) {
        return Ok(None);
    }
    // Not what git said, which names the address, and an address can hold a token.
    Err(git_error(format!("{} failed in {}", found.command, repo.display())))
}
