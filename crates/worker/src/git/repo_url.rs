//! The address a checkout fetches from, which a hub keeps on its repository's record.

use std::path::Path;

use process::Environment;
use protocol::client::ApiError;
use protocol::model::RepoUrl;

use super::{git, git_error};

/// What `git remote` answers for a remote that isn't there.
const NO_SUCH_REMOTE: i32 = 2;

/// The address the checkout at `repo` fetches from, as [`ReadRepoUrl`](protocol::worker::ReadRepoUrl) says: its
/// `origin`'s first URL, as `git remote get-url origin` gives it under the machine's own config, `given`, without the
/// credential it can hold, which is cut out before anything else sees it. `None` when there's no `origin`.
pub async fn repo_url(given: &Environment, repo: &Path) -> Result<Option<RepoUrl>, ApiError> {
    let read = git(given, repo, &["remote", "get-url", "origin"]).await?;
    match read.code {
        Some(0) => Ok(RepoUrl::without_credential(read.stdout.trim())),
        Some(NO_SUCH_REMOTE) => Ok(None),
        // Not what git said, which can name the address, and an address can hold a token. Outside a repository, so a
        // checkout that's gone isn't taken for one with no `origin`.
        _ => Err(git_error(format!("{} failed in {}", read.command, repo.display()))),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use process::Environment;
    use protocol::client::ErrorCode;

    use super::repo_url;
    use crate::git::tests::{new_repo, run};

    async fn read(repo: &Path) -> Option<String> {
        repo_url(&Environment::NONE, repo).await.unwrap().map(String::from)
    }

    /// Where it fetches from, as the person's own rewriting of addresses has it, and not where it pushes.
    #[tokio::test]
    async fn a_checkout_says_where_origin_fetches_from_and_none_without_one() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        run(&repo, &["remote", "add", "upstream", "https://github.com/someone/else.git"]);

        let without = read(&repo).await;
        run(&repo, &["remote", "add", "origin", "ssh://git@example.com/ada/roundhouse.git"]);
        run(&repo, &["config", "--add", "remote.origin.url", "https://example.com/second.git"]);
        run(&repo, &["remote", "set-url", "--push", "origin", "https://example.com/pushed.git"]);
        let with = read(&repo).await;
        run(&repo, &["config", "url.https://github.com/.insteadOf", "gh:"]);
        run(&repo, &["remote", "remove", "origin"]);
        run(&repo, &["remote", "add", "origin", "gh:octo/hello"]);
        let rewritten = read(&repo).await;

        assert_eq!(without, None, "another remote isn't origin");
        assert_eq!(
            with.as_deref(),
            Some("ssh://git@example.com/ada/roundhouse.git"),
            "the first, which git fetches from"
        );
        assert_eq!(rewritten.as_deref(), Some("https://github.com/octo/hello"));
    }

    #[tokio::test]
    async fn a_credential_in_origins_address_is_cut_out_before_it_is_said() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        run(&repo, &["remote", "add", "origin", "https://ada:ghp_made_up@github.com/ada/roundhouse.git"]);

        let said = read(&repo).await;

        assert_eq!(said.as_deref(), Some("https://github.com/ada/roundhouse.git"));
    }

    #[tokio::test]
    async fn a_directory_that_isnt_a_checkout_is_refused_rather_than_said_to_have_none() {
        let dir = tempfile::tempdir().unwrap();

        let read = repo_url(&Environment::NONE, dir.path()).await;

        assert_eq!(read.map_err(|e| e.code), Err(ErrorCode::Worker));
    }
}
