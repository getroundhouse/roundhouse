use super::removal::locked_part_made;
use super::{
    Changing, GitOutput, Worktrees, branches, commit_of, exists, git, git_error, no_branch_in_the_way, off_thread,
    ref_exists, reject_option_like, valid_branch_name,
};
use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::BranchName;
use protocol::worker::WorkspaceReady;
use std::path::Path;
use std::time::Duration;

/// Long enough for a fetch over a slow connection, and short enough that a hung remote doesn't stall making a worktree.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Makes the workspace's worktree from the repository `repo`, its checkout or its clone, under `root`, in a folder of
/// the repository's name.
pub async fn prepare_workspace(
    given: &Environment,
    root: &Path,
    repo_name: &str,
    repo: &Path,
    workspace_name: &str,
    branch: &BranchName,
    base_ref: Option<&str>,
) -> Result<WorkspaceReady, ApiError> {
    let path = root.join(repo_name).join(workspace_name);
    valid_branch_name(given, repo, branch).await?;
    if exists(&path).await {
        return Err(ApiError::new(ErrorCode::NameTaken, format!("{} already exists", path.display())));
    }
    let mut warnings = Vec::new();
    let base = make_on_new_branch(given, repo, branch, base_ref, &path, &mut warnings).await?;
    let branches = branches::read_now(given, &path).await.ok();
    let path = path.to_string_lossy().into_owned();
    Ok(WorkspaceReady { path, branch: branch.clone(), base_ref: base, warnings, branches })
}

/// Makes a worktree at `path` on a new branch `branch` from `base`, or from the repository's default base when there's
/// none, unless git already has a branch in the way. Takes a worktree git made anyway for made, with a warning, and
/// deletes `branch` if git made it and failed without making the worktree. Returns the base it was made from.
pub(super) async fn make_on_new_branch(
    given: &Environment,
    cwd: &Path,
    branch: &BranchName,
    base: Option<&str>,
    path: &Path,
    warnings: &mut Vec<String>,
) -> Result<String, ApiError> {
    let base = async {
        match base {
            Some(base) => Ok(base.to_string()),
            None => default_base(given, cwd).await,
        }
    };
    // Side by side, since none of them changes anything. Their answers are taken in this order, so which refuses
    // doesn't depend on which git finishes first.
    let (base, clear, worktrees) =
        tokio::join!(base, no_branch_in_the_way(given, cwd, branch), Worktrees::of(given, cwd));
    let base = base?;
    reject_option_like(&base, "a base ref")?;
    clear?;
    let worktrees = worktrees?;
    fetch_if_remote(&worktrees, &base, warnings).await?;
    let changing = worktrees.changing().await?;
    let Some(commit) = commit_of(given, cwd, &base).await? else {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{base} isn't a branch, tag or commit here")));
    };
    // --no-track: otherwise a branch made from origin/main tracks it, and a bare `git push` or `git pull` there acts
    // on main. From `base` by name so the branch's reflog records which remote branch it started from.
    let args = ["--no-track", "-b", branch.as_str(), &path.to_string_lossy(), &base];
    let (was_there, added) = add_worktree(&changing, path, &args).await?;
    let branch_taken = added.stderr.contains("a branch named");
    let settled = finish_add(given, cwd, path, branch, was_there, added, warnings).await;
    // Git makes the branch before it looks at the path or checks the files out, so it's there whenever settling the
    // worktree failed unless git refused the branch itself. Only at the commit it was made at, so one moved since isn't
    // deleted.
    if settled.is_err() && !branch_taken {
        let _ = git(given, cwd, &["update-ref", "-d", &format!("refs/heads/{branch}"), commit.as_str()]).await;
    }
    settled.map(|()| base)
}

/// Makes a worktree at `path` on `branch` as it is. Takes a worktree git made anyway for made, with a warning.
pub(super) async fn make_on_branch(
    given: &Environment,
    cwd: &Path,
    branch: &BranchName,
    path: &Path,
    warnings: &mut Vec<String>,
) -> Result<(), ApiError> {
    if !ref_exists(given, cwd, &format!("refs/heads/{branch}")).await? {
        return Err(ApiError::new(ErrorCode::NotFound, format!("there's no branch {branch}")));
    }
    let worktrees = Worktrees::of(given, cwd).await?;
    let changing = worktrees.changing().await?;
    let (was_there, added) = add_worktree(&changing, path, &[&path.to_string_lossy(), branch.as_str()]).await?;
    finish_add(given, cwd, path, branch, was_there, added, warnings).await
}

/// Whether `path` is a finished worktree of the repository at `cwd`, with `branch` checked out.
pub(super) async fn made_there(given: &Environment, cwd: &Path, path: &Path, branch: &BranchName) -> bool {
    if !exists(&path.join(".git")).await {
        return false;
    }
    let dirs = git(given, path, &["rev-parse", "--path-format=absolute", "--git-common-dir", "--git-dir"]);
    let ours = git(given, cwd, &["rev-parse", "--path-format=absolute", "--git-common-dir"]);
    let (Ok(dirs), Ok(ours), Some(head)) = tokio::join!(dirs, ours, branches::checked_out(given, path)) else {
        return false;
    };
    let Some((theirs, record)) = dirs.stdout.trim().split_once('\n').filter(|_| dirs.success && ours.success) else {
        return false;
    };
    theirs == ours.stdout.trim() && head == *branch && !locked_part_made(Path::new(record)).await
}

/// Makes `path`'s parent directory, prunes missing worktrees, notes whether `path` is already there, and runs
/// `git worktree add` with `args`.
async fn add_worktree(changing: &Changing, path: &Path, args: &[&str]) -> Result<(bool, GitOutput), ApiError> {
    make_parent(path).await?;
    changing.prune().await?;
    let was_there = exists(path).await;
    let added = changing.add(args).await?;
    Ok((was_there, added))
}

/// Settles what `git worktree add` did: fine, with a warning, if it made the worktree at `path` anyway, as when a
/// `post-checkout` hook fails after the checkout, and an error for why git refused otherwise.
async fn finish_add(
    given: &Environment,
    cwd: &Path,
    path: &Path,
    branch: &BranchName,
    was_there: bool,
    added: GitOutput,
    warnings: &mut Vec<String>,
) -> Result<(), ApiError> {
    if added.success {
        return Ok(());
    }
    if !was_there && made_there(given, cwd, path, branch).await {
        warnings.push(format!("git made the worktree, but said: {}", added.stderr));
        return Ok(());
    }
    let code = if added.stderr.contains("already exists") || added.stderr.contains("exists; cannot create") {
        // A path or a branch that wasn't there when this worker looked, made meanwhile by something else.
        ErrorCode::NameTaken
    } else if added.stderr.contains("already") {
        // Git lets a branch be checked out in only one worktree.
        ErrorCode::Conflict
    } else {
        ErrorCode::Worker
    };
    let message = if added.stderr.is_empty() { format!("{} failed", added.command) } else { added.stderr };
    Err(ApiError::new(code, message))
}

/// Makes the directory a new worktree goes in.
async fn make_parent(path: &Path) -> Result<(), ApiError> {
    let Some(parent) = path.parent() else { return Ok(()) };
    let made = off_thread(parent.to_path_buf(), std::fs::create_dir_all).await;
    made.map_err(|e| git_error(format!("creating {}: {e}", parent.display())))
}

/// `origin`'s default branch if there's a remote, otherwise the branch checked out in the repo.
async fn default_base(given: &Environment, repo: &Path) -> Result<String, ApiError> {
    let head = git(given, repo, &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"]).await?;
    if head.success && !head.stdout.trim().is_empty() {
        return Ok(head.stdout.trim().to_string());
    }
    // Without one, the rest are asked side by side, and the first of them that's there taken, in this order.
    let (main, master, current) = tokio::join!(
        ref_exists(given, repo, "origin/main"),
        ref_exists(given, repo, "origin/master"),
        git(given, repo, &["symbolic-ref", "--quiet", "--short", "HEAD"]),
    );
    for (candidate, there) in [("origin/main", main), ("origin/master", master)] {
        if there? {
            return Ok(candidate.to_string());
        }
    }
    let current = current?;
    if current.success {
        return Ok(current.stdout.trim().to_string());
    }
    Ok("HEAD".to_string())
}

/// Fetches `base` first if it's one of `origin`'s branches, so a worktree starts from its latest commit even if the
/// repository's copy is behind. A failed fetch isn't fatal, and warns: working offline from the last known commit is
/// better than not working. Holds `worktrees` for reading, since a fetch reads every worktree, and waits for them before
/// its own time limit starts. Fails only if they can't be held, which would fail the worktree's making next.
async fn fetch_if_remote(worktrees: &Worktrees, base: &str, warnings: &mut Vec<String>) -> Result<(), ApiError> {
    let Some(remote_branch) = base.strip_prefix("origin/") else { return Ok(()) };
    let reading = worktrees.reading().await?;
    match tokio::time::timeout(FETCH_TIMEOUT, reading.fetch(&["--quiet", "origin", remote_branch])).await {
        Ok(Ok(output)) if output.success => {}
        Ok(Ok(output)) => warnings.push(format!("couldn't fetch {base}, using the local copy: {}", output.stderr)),
        Ok(Err(e)) => warnings.push(format!("couldn't fetch {base}, using the local copy: {}", e.message)),
        Err(_) => warnings.push(format!("fetching {base} timed out, using the local copy")),
    }
    Ok(())
}
