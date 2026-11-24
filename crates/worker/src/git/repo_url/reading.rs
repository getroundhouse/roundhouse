use std::path::Path;

use process::Environment;
use protocol::client::ApiError;
use protocol::model::RepoUrl;
use protocol::worker::RepoUrlReply;

use crate::git::{git, git_error};

/// What `git remote` answers for a remote that isn't there.
const NO_SUCH_REMOTE: i32 = 2;

/// What [`ReadRepoUrl`](protocol::worker::ReadRepoUrl) says of the checkout at `repo`: its `origin`'s first URL, as
/// `git remote get-url origin` gives it under the machine's own config, `machine`, without the credential it can hold,
/// which is cut out before anything else sees it, or none when there's no `origin`. Never where its pushes go, which
/// no hub of this build reads.
pub async fn repo_url(machine: &Environment, repo: &Path) -> Result<RepoUrlReply, ApiError> {
    let read = git(machine, repo, &["remote", "get-url", "origin"]).await?;
    let url = match read.status.code() {
        Some(0) => RepoUrl::without_credential(read.stdout.trim()),
        Some(NO_SUCH_REMOTE) => return Ok(RepoUrlReply { url: None, push: None }),
        // Not what git said, which can name the address, and an address can hold a token. Outside a repository, so a
        // checkout that's gone isn't taken for one with no `origin`.
        _ => return Err(git_error(format!("{} failed in {}", read.command, repo.display()))),
    };
    Ok(RepoUrlReply { url, push: None })
}
