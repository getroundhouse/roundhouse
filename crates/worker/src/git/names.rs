//! Which names git has taken in a repository: the branches a new one can't be made beside, and the names a new
//! workspace can't have, since its branches go in a folder of its name, which is its own.

use std::collections::BTreeSet;
use std::path::Path;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::BranchName;

use super::git_ok;

const BRANCHES: &str = "refs/heads/";
const ORIGIN_BRANCHES: &str = "refs/remotes/origin/";

/// The names git already has in the repository, as [`NamesGitHas`](protocol::worker::NamesGitHas) says, each once, as
/// git spells it.
pub async fn names_git_has(root: &Path, repo_name: &str, repo_path: &str) -> Result<Vec<String>, ApiError> {
    let repo = Path::new(repo_path);
    let listed = refs(repo, &[BRANCHES, ORIGIN_BRANCHES]).await?;
    let branches =
        listed.iter().filter_map(|name| name.strip_prefix(BRANCHES).or_else(|| name.strip_prefix(ORIGIN_BRANCHES)));
    let mut names: BTreeSet<String> = branches.filter_map(|name| name.split('/').next()).map(str::to_string).collect();
    // A branch in a folder named after a remote, as `origin/main` would be, is found before the remote's own.
    let remotes = git_ok(repo, &["remote"]).await?;
    names.extend(remotes.lines().map(str::to_string));
    if !remotes.trim().is_empty() {
        // Each remote has a `HEAD`, or gets one at a later fetch, which is no branch but takes the name all the same:
        // where case isn't told apart, a branch `head/start` pushed there would need a folder where that file is, and
        // every fetch after would fail.
        names.insert("HEAD".to_string());
    }
    let workspaces = root.join(repo_name);
    // Nothing is there until the repository's first workspace is, and a directory that can't be listed takes no
    // name here: making the worktree still refuses one that exists.
    let directories = tokio::task::spawn_blocking(move || {
        let entries = std::fs::read_dir(workspaces).into_iter().flatten().flatten();
        entries.filter_map(|entry| entry.file_name().into_string().ok()).collect::<Vec<_>>()
    });
    names.extend(directories.await.unwrap_or_default());
    Ok(names.into_iter().collect())
}

/// Refuses, as `NameTaken`, a new branch git has one in the way of: one of its name, or one it would be a folder of or
/// in, as `holly` is for `holly/start`. Whatever their case, since git keeps a branch as a file, and a file system that
/// doesn't tell case apart takes `Holly/start` for `holly/start`.
pub(super) async fn no_branch_in_the_way(repo: &Path, branch: &BranchName) -> Result<(), ApiError> {
    let wanted = branch.as_str().to_ascii_lowercase();
    let clashes = |had: &&str| {
        let had = had.to_ascii_lowercase();
        had == wanted || wanted.starts_with(&format!("{had}/")) || had.starts_with(&format!("{wanted}/"))
    };
    let listed = refs(repo, &[BRANCHES]).await?;
    match listed.iter().filter_map(|name| name.strip_prefix(BRANCHES)).find(clashes) {
        Some(taken) => Err(ApiError::new(ErrorCode::NameTaken, format!("branch {taken} already exists"))),
        None => Ok(()),
    }
}

/// Every ref under each of `under`, by its full name.
async fn refs(repo: &Path, under: &[&str]) -> Result<Vec<String>, ApiError> {
    let mut args = vec!["for-each-ref", "--format=%(refname)"];
    args.extend_from_slice(under);
    Ok(git_ok(repo, &args).await?.lines().map(str::to_string).collect())
}
