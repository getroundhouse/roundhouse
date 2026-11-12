//! Making another worktree for a workspace's work, in `.children/` beside the workspaces, as an [`AddChildWorktree`]
//! asks: what git does, and what's left when it fails part way. Removing one is removing any worktree, as archiving
//! does.
//!
//! [`AddChildWorktree`]: protocol::worker::AddChildWorktree

use std::path::{Component, Path};

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{BranchName, CHILDREN, CommitId};
use protocol::worker::ChildBranch;

use super::removal::remove_part_made;
use super::{
    GitOutput, commit_of, fetch_if_remote, git, git_ok, make_parent, no_branch_in_the_way, ref_exists,
    reject_option_like, valid_branch_name,
};

#[cfg(test)]
mod tests;

/// Makes the worktree at `path`, another of the workspace whose own is at `own`, and says which commit it has
/// checked out. One there already on the branch asked for is what was asked for, made by this ask heard before, or by
/// hand. Whoever calls has `path` to itself.
pub(crate) async fn add_child_worktree(
    own: &Path,
    path: &Path,
    on: &ChildBranch,
    warnings: &mut Vec<String>,
) -> Result<CommitId, ApiError> {
    in_its_folder(own, path)?;
    let branch = match on {
        ChildBranch::New { branch, .. } | ChildBranch::Existing { branch } => branch,
    };
    // Run in the workspace's own worktree, as everything here is, so `HEAD` is the commit it has checked out.
    valid_branch_name(own, branch).await?;
    // One a `git worktree add` was stopped part way through, as when the worker went, is nothing git will finish, so
    // it's removed, and made again below.
    let had = checked_out(path).await;
    let stopped_part_way = remove_part_made(own, path).await?;
    // Its folder being a worktree itself, as one made there by hand would be, would have this one made inside it, and
    // go with it when that one is removed.
    let inside_another = path.parent().is_some_and(|folder| folder.join(".git").exists());
    if inside_another || (!stopped_part_way && path.exists() && !made_there(own, path, branch).await) {
        return Err(ApiError::new(ErrorCode::NameTaken, format!("{} is taken", path.display())));
    }
    if !path.exists() {
        // On the branch git had checked out in it before it was stopped, if that's the one asked for, rather than
        // refusing that as a new branch that's taken.
        let its_own = ChildBranch::Existing { branch: branch.clone() };
        let on_its_own = stopped_part_way && had == Some(format!("refs/heads/{branch}"));
        make(own, if on_its_own { &its_own } else { on }, path, warnings).await?;
    }
    commit_of(path, "HEAD")
        .await?
        .ok_or_else(|| ApiError::new(ErrorCode::Worker, format!("{} has no commit checked out", path.display())))
}

/// Refuses a `path` that isn't a directory of its own in the workspace's folder of the `.children/` beside its own
/// worktree at `own`, the folder named as that worktree's directory is. So a hub can't have a worktree made outside
/// `.children/`, in another workspace's folder, inside another worktree, or where archiving moves worktrees aside.
fn in_its_folder(own: &Path, path: &Path) -> Result<(), ApiError> {
    let folder = own.parent().unwrap_or(own).join(CHILDREN).join(own.file_name().unwrap_or_default());
    let named =
        |part: Component<'_>| matches!(part, Component::Normal(name) if !name.to_string_lossy().starts_with('.'));
    let mut inside = path.strip_prefix(&folder).map(Path::components).into_iter().flatten();
    if !inside.next().is_some_and(named) || inside.next().is_some() {
        let refused = format!("{} isn't where one of this workspace's other worktrees goes", path.display());
        return Err(ApiError::new(ErrorCode::InvalidRequest, refused));
    }
    Ok(())
}

/// Runs `git worktree add` for `path`, which is fine, with a warning, if git made the worktree anyway, as when a
/// `post-checkout` hook fails after it's checked out.
async fn make(own: &Path, on: &ChildBranch, path: &Path, warnings: &mut Vec<String>) -> Result<(), ApiError> {
    // A worktree deleted without git, as by an agent's `rm -rf`, stays registered, and would refuse its path and
    // its branch.
    git_ok(own, &["worktree", "prune"]).await?;
    let added = match on {
        ChildBranch::New { branch, base } => on_a_new_branch(own, branch, base, path, warnings).await?,
        ChildBranch::Existing { branch } => on_a_branch(own, branch, path).await?,
    };
    if added.success {
        return Ok(());
    }
    if path.join(".git").exists() {
        warnings.push(format!("git made the worktree, but said: {}", added.stderr));
        return Ok(());
    }
    let code = if added.stderr.contains("already exists") {
        // A path or a branch that wasn't there when this worker looked, made meanwhile by something else.
        ErrorCode::NameTaken
    } else if added.stderr.contains("already") {
        // Git lets a branch be checked out in only one worktree.
        ErrorCode::Conflict
    } else {
        ErrorCode::Worker
    };
    Err(ApiError::new(code, added.stderr))
}

/// Whether `path` is a worktree of the repository of the workspace's own at `own`, with `branch` checked out.
async fn made_there(own: &Path, path: &Path, branch: &BranchName) -> bool {
    if !path.join(".git").exists() {
        return false;
    }
    let repository = |cwd| git(cwd, &["rev-parse", "--path-format=absolute", "--git-common-dir"]);
    let said = |output: Result<GitOutput, ApiError>| {
        output.ok().filter(|output| output.success).map(|output| output.stdout.trim().to_string())
    };
    let (theirs, ours, head) = tokio::join!(repository(path), repository(own), checked_out(path));
    let theirs = said(theirs);
    theirs.is_some() && theirs == said(ours) && head == Some(format!("refs/heads/{branch}"))
}

/// The branch the worktree at `path` has checked out, in full, since git shortens one to `heads/x` when a tag is called
/// `x` too. `None` if no worktree is there, or it's on no branch.
async fn checked_out(path: &Path) -> Option<String> {
    if !path.join(".git").exists() {
        return None;
    }
    let head = git(path, &["symbolic-ref", "--quiet", "HEAD"]).await.ok()?;
    head.success.then(|| head.stdout.trim().to_string())
}

/// Runs `git worktree add` for `path` on a new branch from `base`, unless git already has a branch in the way.
async fn on_a_new_branch(
    own: &Path,
    branch: &BranchName,
    base: &str,
    path: &Path,
    warnings: &mut Vec<String>,
) -> Result<GitOutput, ApiError> {
    reject_option_like(base, "a base ref")?;
    no_branch_in_the_way(own, branch).await?;
    fetch_if_remote(own, base, warnings).await;
    let Some(commit) = commit_of(own, base).await? else {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{base} isn't a branch, tag or commit here")));
    };
    make_parent(path).await?;
    // From the commit, not the base's name, and --no-track, as for a workspace's own, so the branch tracks nothing,
    // and a bare `git push` or `git pull` there doesn't act on the base.
    let added =
        git(own, &["worktree", "add", "--no-track", "-b", branch.as_str(), &path.to_string_lossy(), commit.as_str()])
            .await?;
    // Git makes the branch before it looks at the path, so it's there unless git refused it as one that was.
    if !added.success && !path.join(".git").exists() && !added.stderr.contains("a branch named") {
        // It holds nothing else, unless it's moved since.
        let _ = git(own, &["update-ref", "-d", &format!("refs/heads/{branch}"), commit.as_str()]).await;
    }
    Ok(added)
}

/// Runs `git worktree add` for `path` on `branch` as it is.
async fn on_a_branch(own: &Path, branch: &BranchName, path: &Path) -> Result<GitOutput, ApiError> {
    if !ref_exists(own, &format!("refs/heads/{branch}")).await? {
        return Err(ApiError::new(ErrorCode::NotFound, format!("there's no branch {branch}")));
    }
    make_parent(path).await?;
    git(own, &["worktree", "add", &path.to_string_lossy(), branch.as_str()]).await
}
