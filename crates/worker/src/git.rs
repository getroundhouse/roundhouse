//! Worktrees and git state, by running the `git` binary. Local config, credentials and hooks then
//! behave exactly as they do for the user.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{CommitId, GitDiff, GitFile, GitStatus};
use tokio::process::Command;

pub(crate) mod blob;
pub(crate) mod files;
mod pull_request;
mod removal;
#[cfg(test)]
pub(crate) mod tests;

pub use pull_request::{pull_request, rate_limit};
pub use removal::remove_workspace;

const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const GH_TIMEOUT: Duration = Duration::from_secs(15);
/// Keeps a diff response bounded when an agent leaves build output untracked and unignored.
const MAX_UNTRACKED_FILES: usize = 200;
const MAX_UNTRACKED_BYTES: usize = 8 * 1024 * 1024;

pub struct Prepared {
    pub path: String,
    pub branch: String,
    pub base_ref: String,
    pub warnings: Vec<String>,
}

fn git_error(message: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::Worker, message)
}

struct GitOutput {
    success: bool,
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Runs git, with what it prints as it printed it.
async fn run(cwd: &Path, args: &[&str]) -> Result<std::process::Output, ApiError> {
    command(cwd, args).output().await.map_err(|e| git_error(format!("could not run git in {}: {e}", cwd.display())))
}

/// Git, ready to start, as everything here starts it.
fn command(cwd: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(cwd)
        // Never wait on a credentials prompt nobody can see.
        .env("GIT_TERMINAL_PROMPT", "0")
        // Status checks shouldn't take locks that make the agent's own git commands fail.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        // So a fetch that times out doesn't keep running, and nor does a read that's had what it needs.
        .kill_on_drop(true);
    command
}

async fn git(cwd: &Path, args: &[&str]) -> Result<GitOutput, ApiError> {
    let output = run(cwd, args).await?;
    Ok(GitOutput {
        success: output.status.success(),
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

async fn git_ok(cwd: &Path, args: &[&str]) -> Result<String, ApiError> {
    let output = git(cwd, args).await?;
    if output.success {
        Ok(output.stdout)
    } else {
        Err(git_error(format!("git {} failed: {}", args.join(" "), output.stderr)))
    }
}

/// For commands that print NUL-separated paths, such as `git ls-files -z`.
pub(crate) async fn list_nul_separated(repo: &Path, args: &[String]) -> Result<Vec<String>, ApiError> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = git_ok(repo, &args).await?;
    Ok(output.split('\0').filter(|f| !f.is_empty()).map(str::to_string).collect())
}

async fn ref_exists(repo: &Path, name: &str) -> Result<bool, ApiError> {
    Ok(git(repo, &["rev-parse", "--verify", "--quiet", &format!("{name}^{{commit}}")]).await?.success)
}

/// `origin`'s default branch if there's a remote, otherwise the branch checked out in the repo.
async fn default_base(repo: &Path) -> Result<String, ApiError> {
    let head = git(repo, &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"]).await?;
    if head.success && !head.stdout.trim().is_empty() {
        return Ok(head.stdout.trim().to_string());
    }
    for candidate in ["origin/main", "origin/master"] {
        if ref_exists(repo, candidate).await? {
            return Ok(candidate.to_string());
        }
    }
    let current = git(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"]).await?;
    if current.success {
        return Ok(current.stdout.trim().to_string());
    }
    Ok("HEAD".to_string())
}

fn reject_option_like(value: &str, what: &str) -> Result<(), ApiError> {
    if value.starts_with('-') {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{what} can't start with '-': {value}")));
    }
    Ok(())
}

pub async fn prepare_workspace(
    root: &Path,
    repo_name: &str,
    repo_path: &str,
    workspace_name: &str,
    branch: &str,
    base_ref: Option<&str>,
) -> Result<Prepared, ApiError> {
    reject_option_like(branch, "a branch name")?;
    let repo = PathBuf::from(repo_path);
    let path = root.join(repo_name).join(workspace_name);
    if path.exists() {
        return Err(ApiError::new(ErrorCode::NameTaken, format!("{} already exists", path.display())));
    }
    if ref_exists(&repo, &format!("refs/heads/{branch}")).await? {
        return Err(ApiError::new(ErrorCode::NameTaken, format!("branch {branch} already exists")));
    }
    if !git(&repo, &["check-ref-format", "--branch", branch]).await?.success {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{branch} isn't a valid branch name")));
    }

    let base = match base_ref {
        Some(base) => {
            reject_option_like(base, "a base ref")?;
            base.to_string()
        }
        None => default_base(&repo).await?,
    };

    // Start from the latest remote commit even if the local checkout is behind. A failed fetch
    // isn't fatal: working offline from the last known commit is better than not working.
    let mut warnings = Vec::new();
    if let Some(remote_branch) = base.strip_prefix("origin/") {
        let args = ["fetch", "--quiet", "origin", remote_branch];
        match tokio::time::timeout(FETCH_TIMEOUT, git(&repo, &args)).await {
            Ok(Ok(output)) if output.success => {}
            Ok(Ok(output)) => warnings.push(format!("couldn't fetch {base}, using the local copy: {}", output.stderr)),
            Ok(Err(e)) => warnings.push(format!("couldn't fetch {base}, using the local copy: {}", e.message)),
            Err(_) => warnings.push(format!("fetching {base} timed out, using the local copy")),
        }
    }
    if !ref_exists(&repo, &base).await? {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("base {base} doesn't exist in {repo_path}")));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| git_error(format!("creating {}: {e}", parent.display())))?;
    }
    let path_str = path.to_string_lossy().into_owned();
    // --no-track: otherwise a branch made from origin/main tracks it, and a bare `git push` or
    // `git pull` from the workspace would act on main.
    git_ok(&repo, &["worktree", "add", "--no-track", "-b", branch, &path_str, &base]).await?;
    Ok(Prepared { path: path_str, branch: branch.to_string(), base_ref: base, warnings })
}

pub async fn restore_workspace(repo_path: &str, path: &str, branch: &str) -> Result<(), ApiError> {
    let repo = PathBuf::from(repo_path);
    if Path::new(path).exists() {
        return Err(ApiError::new(ErrorCode::Conflict, format!("{path} already exists")));
    }
    if !ref_exists(&repo, &format!("refs/heads/{branch}")).await? {
        return Err(ApiError::new(ErrorCode::Conflict, format!("branch {branch} no longer exists")));
    }
    // A worktree deleted outside this app stays registered, and would block re-adding this path.
    git_ok(&repo, &["worktree", "prune"]).await?;
    git_ok(&repo, &["worktree", "add", path, branch]).await?;
    Ok(())
}

async fn any_ref_holds_head(worktree: &Path) -> Result<bool, ApiError> {
    let holders = git_ok(worktree, &["for-each-ref", "--contains", "HEAD", "--count=1"]).await?;
    Ok(!holders.trim().is_empty())
}

pub async fn status(path: &str) -> Result<GitStatus, ApiError> {
    // Explicit, because `status.showUntrackedFiles=no` in someone's config would otherwise hide new
    // files from this, and from the check that keeps archiving from deleting them.
    let args = ["status", "--porcelain=v2", "--branch", "--untracked-files=normal", "-z"];
    let worktree = Path::new(path);
    let output = git_ok(worktree, &args).await?;
    let mut status = parse_status(&output);
    status.unreferenced_commits = status.branch.is_none() && !any_ref_holds_head(worktree).await?;
    Ok(status)
}

fn parse_status(output: &str) -> GitStatus {
    let mut status = GitStatus::default();
    let mut fields = output.split('\0');
    while let Some(entry) = fields.next() {
        if let Some(header) = entry.strip_prefix("# ") {
            let mut parts = header.split(' ');
            match parts.next() {
                Some("branch.head") => status.branch = parts.next().filter(|b| *b != "(detached)").map(String::from),
                Some("branch.oid") => status.head = parts.next().filter(|o| *o != "(initial)").map(String::from),
                Some("branch.ab") => {
                    let count = |part: Option<&str>| part.and_then(|p| p[1..].parse().ok()).unwrap_or(0);
                    status.ahead = count(parts.next());
                    status.behind = count(parts.next());
                }
                _ => {}
            }
        } else if let Some(path) = entry.strip_prefix("? ") {
            status.files.push(GitFile { path: path.to_string(), orig_path: None, status: "??".to_string() });
        } else if entry.starts_with("1 ") {
            let parts: Vec<&str> = entry.splitn(9, ' ').collect();
            if let [_, xy, .., path] = parts.as_slice() {
                status.files.push(GitFile { path: path.to_string(), orig_path: None, status: xy.to_string() });
            }
        } else if entry.starts_with("2 ") {
            let parts: Vec<&str> = entry.splitn(10, ' ').collect();
            if let [_, xy, .., path] = parts.as_slice() {
                // With -z, the original path is the next NUL-separated field.
                let orig_path = fields.next().map(String::from);
                status.files.push(GitFile { path: path.to_string(), orig_path, status: xy.to_string() });
            }
        } else if entry.starts_with("u ") {
            let parts: Vec<&str> = entry.splitn(11, ' ').collect();
            if let [_, xy, .., path] = parts.as_slice() {
                status.files.push(GitFile { path: path.to_string(), orig_path: None, status: xy.to_string() });
            }
        }
    }
    status
}

/// Everything the branch changes against its base, including uncommitted and untracked files.
pub async fn diff(worktree_path: &str, base_ref: Option<&str>) -> Result<GitDiff, ApiError> {
    let worktree = Path::new(worktree_path);
    let base = diff_base(worktree, base_ref).await?;
    // With no commit at HEAD, git refuses to diff from it, and so there's no diff.
    let from = base.as_ref().map_or("HEAD", CommitId::as_str);
    let mut patch = git_ok(worktree, &["diff", "--no-color", "--no-ext-diff", from]).await?;

    let untracked = git_ok(worktree, &["ls-files", "--others", "--exclude-standard", "-z"]).await?;
    let mut truncated = false;
    for (i, file) in untracked.split('\0').filter(|f| !f.is_empty()).enumerate() {
        if i >= MAX_UNTRACKED_FILES || patch.len() > MAX_UNTRACKED_BYTES {
            truncated = true;
            break;
        }
        // --no-index exits 1 when the files differ, so success here doesn't mean exit 0.
        let output =
            git(worktree, &["diff", "--no-color", "--no-ext-diff", "--no-index", "--", "/dev/null", file]).await?;
        if output.code == Some(0) || output.code == Some(1) {
            patch.push_str(&output.stdout);
        } else {
            return Err(git_error(format!("diffing untracked file {file} failed: {}", output.stderr)));
        }
    }
    Ok(GitDiff { base_commit: base, patch, truncated })
}

/// The commit a diff is taken from, by its full object ID, so a client can read a file as it was there: the merge base
/// with `base_ref`, or HEAD's commit when there's no base ref or git finds no merge base with it. `None` only when
/// HEAD has no commit yet.
async fn diff_base(worktree: &Path, base_ref: Option<&str>) -> Result<Option<CommitId>, ApiError> {
    if let Some(base) = base_ref {
        let merge_base = git(worktree, &["merge-base", base, "HEAD"]).await?;
        if merge_base.success {
            return Ok(Some(CommitId::from(merge_base.stdout.trim())));
        }
    }
    let head = git(worktree, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]).await?;
    Ok(head.success.then(|| CommitId::from(head.stdout.trim())))
}
