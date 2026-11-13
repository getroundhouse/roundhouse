use crate::git::{commit_of, git, git_error, git_ok};
use protocol::client::ApiError;
use protocol::model::{BranchName, CommitId};
use std::collections::HashSet;
use std::path::Path;
use tracing::warn;

/// The branch's setting that holds the link, as `branch.<name>.roundhouse-pull-request`.
const SETTING: &str = "roundhouse-pull-request";

/// What to make of the pull request `gh` found by the branch's name.
#[derive(Debug, PartialEq, Eq)]
pub(in super::super) enum Choice {
    /// It's the one the branch is linked to.
    Found,
    /// It's the branch's, and the branch isn't linked to it yet.
    FoundAndLink,
    /// It only shares the name, or there's none, so the branch's is the one it's linked to, read by its URL.
    ReadLinked,
    /// The branch has none.
    None,
}

/// From the URL of the one `gh` found, the URL of the one the branch is linked to, and whether the branch has been at
/// one of the found one's commits, which is asked only when it isn't the one linked.
pub(in super::super) fn choose(found: Option<&str>, linked: Option<&str>, has_been_at: bool) -> Choice {
    match (found, linked) {
        (Some(found), Some(linked)) if found == linked => Choice::Found,
        (Some(_), _) if has_been_at => Choice::FoundAndLink,
        (_, Some(_)) => Choice::ReadLinked,
        (_, None) => Choice::None,
    }
}

/// The URL of the pull request `branch` is linked to, or `None` when it's linked to none. An error when git couldn't
/// say, which isn't the same.
pub(in super::super) async fn linked(worktree: &Path, branch: &BranchName) -> Result<Option<String>, ApiError> {
    // The repository's own config, which is where a branch's settings are, and nowhere else's word for it.
    let read = git(worktree, &["config", "--local", "--get", &setting(branch)]).await?;
    match read.code {
        // Only a URL, since it's passed to `gh`, which would take anything else for a branch or a flag.
        Some(0) => Ok(Some(read.stdout.trim().to_string()).filter(|url| url.starts_with("http"))),
        // git's word for a setting that isn't there.
        Some(1) => Ok(None),
        _ => Err(git_error(format!("git couldn't say which pull request is {branch}'s: {}", read.stderr))),
    }
}

/// Links `branch` to the pull request at `url`, in place of any it was linked to. Only when that changes it, since
/// every worktree of the repository shares its config, and writing it takes a lock that would make an agent's own
/// write, such as `git push -u`'s, fail meanwhile. One that can't be written is only missed once the pull request
/// moves on without any of the branch's commits.
pub(in super::super) async fn link(worktree: &Path, branch: &BranchName, url: &str) {
    let written = git_ok(worktree, &["config", "--local", "--replace-all", &setting(branch), url]).await;
    if let Err(e) = written {
        warn!(%branch, "couldn't link the branch to its pull request: {}", e.message);
    }
}

/// Whether `branch` is at one of `commits`, the commits of a pull request from `its_branch`, or its history says it
/// has been. Not at the commit it was made at, when it was made from a remote's branch other than that one, as a
/// workspace is made from `origin/main` or a teammate's branch, since a branch of the same name elsewhere can be made
/// from it too, and a pull request from there into another base has its commits. A branch with no commits yet has
/// been at none. An error when git couldn't say.
pub(in super::super) async fn has_been_at(
    worktree: &Path,
    branch: &BranchName,
    commits: &[CommitId],
    its_branch: Option<&BranchName>,
) -> Result<bool, ApiError> {
    let own = format!("refs/heads/{branch}");
    let Some(tip) = commit_of(worktree, &own).await? else { return Ok(false) };
    let commits: HashSet<&str> = commits.iter().map(CommitId::as_str).collect();
    let history = git_ok(worktree, &["reflog", "show", "--format=%H%x00%gs", &own, "--"]).await?;
    let history: Vec<(&str, &str)> = history.lines().filter_map(|entry| entry.split_once('\0')).collect();
    // Empty in a repository that keeps no reflogs, where only where the branch is now says.
    let Some(&(made_at, made_how)) = history.last() else { return Ok(commits.contains(tip.as_str())) };
    let shared_start = commits.contains(made_at)
        && match made_from(made_how) {
            Some(start) => remote_branch(start, &remotes(worktree).await?)
                .is_some_and(|name| Some(name) != its_branch.map(BranchName::as_str)),
            None => false,
        };
    Ok(history.iter().any(|&(at, _)| commits.contains(at) && !(shared_start && at == made_at)))
}

/// What a branch was made from, from the subject of the oldest entry in its reflog, such as `origin/main` or
/// `refs/remotes/origin/birch`. `None` for one made another way, as by a fetch straight into it.
pub(in super::super) fn made_from(subject: &str) -> Option<&str> {
    subject.strip_prefix("branch: Created from ")
}

/// The name of the remote's branch `start` is, of one of `remotes`, or `None` when it's no remote's, as `main` or
/// `HEAD` isn't. A remote's name can have a slash in it, as a branch's can, so the longest that fits is the one.
pub(in super::super) fn remote_branch<'a>(start: &'a str, remotes: &[String]) -> Option<&'a str> {
    let start = start.trim_prefix("refs/remotes/");
    let names = remotes.iter().filter_map(|remote| start.strip_prefix(remote.as_str())?.strip_prefix('/'));
    names.min_by_key(|name| name.len())
}

async fn remotes(worktree: &Path) -> Result<Vec<String>, ApiError> {
    Ok(git_ok(worktree, &["remote"]).await?.lines().map(str::to_string).collect())
}

fn setting(branch: &BranchName) -> String {
    format!("branch.{branch}.{SETTING}")
}
