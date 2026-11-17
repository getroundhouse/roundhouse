use std::path::Path;

use process::Environment;
use protocol::client::ApiError;
use protocol::model::RepoUrl;
use protocol::worker::RepoUrlReply;

use super::route::route;
use crate::git::{git, git_error};
use crate::git_credential::rewriting;

/// What `git remote` answers for a remote that isn't there.
const NO_SUCH_REMOTE: i32 = 2;

/// What [`ReadRepoUrl`](protocol::worker::ReadRepoUrl) says of the checkout at `repo`: its `origin`'s first URL, as
/// `git remote get-url origin` gives it under the machine's own config, `machine`, without the credential it can hold,
/// which is cut out before anything else sees it, and where a push from it goes, as
/// [`PushRoute`](protocol::worker::PushRoute) says, by its push addresses under `machine` with the worker's rewriting
/// of GitHub's. None of either when there's no `origin`.
pub async fn repo_url(machine: &Environment, repo: &Path) -> Result<RepoUrlReply, ApiError> {
    let read = git(machine, repo, &["remote", "get-url", "origin"]).await?;
    let url = match read.code {
        Some(0) => RepoUrl::without_credential(read.stdout.trim()),
        Some(NO_SUCH_REMOTE) => return Ok(RepoUrlReply { url: None, push: None }),
        // Not what git said, which can name the address, and an address can hold a token. Outside a repository, so a
        // checkout that's gone isn't taken for one with no `origin`.
        _ => return Err(git_error(format!("{} failed in {}", read.command, repo.display()))),
    };
    let pushes = git(&rewriting(machine), repo, &["remote", "get-url", "--push", "--all", "origin"]).await?;
    if !pushes.success {
        return Err(git_error(format!("{} failed in {}", pushes.command, repo.display())));
    }
    Ok(RepoUrlReply { url, push: Some(route(&pushes.stdout)) })
}
