//! Worktrees and git state, by running the `git` binary. Local config, credentials and hooks then
//! behave exactly as they do for the user.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GitDiff, GitFile, GitStatus};
use tokio::process::Command;

const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
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

async fn git(cwd: &Path, args: &[&str]) -> Result<GitOutput, ApiError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        // Never wait on a credentials prompt nobody can see.
        .env("GIT_TERMINAL_PROMPT", "0")
        // Status checks shouldn't take locks that make the agent's own git commands fail.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        // So a fetch that times out doesn't keep running.
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| git_error(format!("could not run git in {}: {e}", cwd.display())))?;
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
        return Err(ApiError::new(ErrorCode::Conflict, format!("{} already exists", path.display())));
    }
    if ref_exists(&repo, &format!("refs/heads/{branch}")).await? {
        return Err(ApiError::new(ErrorCode::Conflict, format!("branch {branch} already exists")));
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

/// Keeps the branch, so committed work is never lost by archiving.
pub async fn remove_workspace(repo_path: &str, path: &str, force: bool) -> Result<(), ApiError> {
    let repo = PathBuf::from(repo_path);
    let worktree = Path::new(path);
    if !worktree.exists() {
        // Already gone, perhaps deleted by hand. Only git's record of it is left to clean up.
        git_ok(&repo, &["worktree", "prune"]).await?;
        return Ok(());
    }
    if !force {
        let current = status(path).await?;
        if !current.files.is_empty() {
            return Err(ApiError::new(
                ErrorCode::Conflict,
                format!(
                    "{} uncommitted change(s) in {path} would be lost. Commit them, or archive with force",
                    current.files.len()
                ),
            ));
        }
        if current.branch.is_none() && !any_ref_holds_head(worktree).await? {
            return Err(ApiError::new(
                ErrorCode::Conflict,
                format!(
                    "{path} is on a detached HEAD with commits on no branch, which would be lost. \
                     Put them on a branch, or archive with force"
                ),
            ));
        }
    }
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(path);
    git_ok(&repo, &args).await?;
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
    let output = git_ok(Path::new(path), &args).await?;
    Ok(parse_status(&output))
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
pub async fn diff(path: &str, base_ref: Option<&str>) -> Result<GitDiff, ApiError> {
    let worktree = Path::new(path);
    let base = match base_ref {
        Some(base) => {
            let merge_base = git(worktree, &["merge-base", base, "HEAD"]).await?;
            merge_base.success.then(|| merge_base.stdout.trim().to_string())
        }
        None => None,
    };
    let from = base.clone().unwrap_or_else(|| "HEAD".to_string());
    let mut patch = git_ok(worktree, &["diff", "--no-color", "--no-ext-diff", &from]).await?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn run(cwd: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn new_repo(dir: &Path) -> PathBuf {
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        std::fs::write(repo.join(".gitignore"), "ignored/\n").unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "-qm", "first"]);
        repo
    }

    async fn prepare(root: &Path, repo: &Path, name: &str, base: Option<&str>) -> Result<Prepared, ApiError> {
        prepare_workspace(root, "repo", repo.to_str().unwrap(), name, name, base).await
    }

    #[tokio::test]
    async fn creates_a_worktree_on_a_new_branch_from_the_current_branch() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        let root = dir.path().join("ws");
        let prepared = prepare(&root, &repo, "alder", None).await.unwrap();
        assert_eq!(prepared.base_ref, "main");
        assert_eq!(run(Path::new(&prepared.path), &["branch", "--show-current"]), "alder");
        assert!(Path::new(&prepared.path).join("a.txt").exists());
    }

    #[tokio::test]
    async fn fetches_the_latest_remote_commit_and_does_not_track_the_base() {
        let dir = tempfile::tempdir().unwrap();
        let origin = new_repo(dir.path());
        let clone = dir.path().join("clone");
        run(dir.path(), &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);
        // origin moves on after the clone, so only a fetch can find this commit.
        std::fs::write(origin.join("b.txt"), "new\n").unwrap();
        run(&origin, &["add", "."]);
        run(&origin, &["commit", "-qm", "second"]);
        let latest = run(&origin, &["rev-parse", "HEAD"]);

        let prepared = prepare(&dir.path().join("ws"), &clone, "alder", None).await.unwrap();
        assert_eq!(prepared.base_ref, "origin/main");
        assert!(prepared.warnings.is_empty(), "{:?}", prepared.warnings);
        let path = Path::new(&prepared.path);
        assert_eq!(run(path, &["rev-parse", "HEAD"]), latest);
        // A tracking branch would make a bare `git pull` or `git push` act on main.
        let upstream = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
            .current_dir(path)
            .output()
            .unwrap();
        assert!(!upstream.status.success());
    }

    #[tokio::test]
    async fn an_unreachable_remote_warns_and_uses_the_local_copy() {
        let dir = tempfile::tempdir().unwrap();
        let origin = new_repo(dir.path());
        let clone = dir.path().join("clone");
        run(dir.path(), &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);
        std::fs::remove_dir_all(&origin).unwrap();
        let prepared = prepare(&dir.path().join("ws"), &clone, "alder", None).await.unwrap();
        assert_eq!(prepared.warnings.len(), 1, "{:?}", prepared.warnings);
        assert!(Path::new(&prepared.path).join("a.txt").exists());
    }

    #[tokio::test]
    async fn existing_branches_paths_and_bad_input_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        let root = dir.path().join("ws");
        prepare(&root, &repo, "alder", None).await.unwrap();
        // Same path and branch.
        assert_eq!(prepare(&root, &repo, "alder", None).await.err().unwrap().code, ErrorCode::Conflict);
        // Same branch, different path.
        let err = prepare_workspace(&root, "repo", repo.to_str().unwrap(), "birch", "alder", None).await.err().unwrap();
        assert_eq!(err.code, ErrorCode::Conflict);
        let err = prepare(&root, &repo, "cedar", Some("--upload-pack=evil")).await.err().unwrap();
        assert_eq!(err.code, ErrorCode::InvalidRequest);
        let err = prepare(&root, &repo, "cedar", Some("no-such-ref")).await.err().unwrap();
        assert_eq!(err.code, ErrorCode::InvalidRequest);
        let err = prepare_workspace(&root, "repo", repo.to_str().unwrap(), "x", "--bad", None).await.err().unwrap();
        assert_eq!(err.code, ErrorCode::InvalidRequest);
    }

    #[tokio::test]
    async fn archiving_refuses_uncommitted_work_unless_forced_and_keeps_the_branch() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
        let path = Path::new(&prepared.path);
        let repo_str = repo.to_str().unwrap();

        std::fs::write(path.join("new.txt"), "work\n").unwrap();
        let err = remove_workspace(repo_str, &prepared.path, false).await.err().unwrap();
        assert_eq!(err.code, ErrorCode::Conflict);
        assert!(path.join("new.txt").exists(), "a refused archive must not touch files");

        remove_workspace(repo_str, &prepared.path, true).await.unwrap();
        assert!(!path.exists());
        assert!(!run(&repo, &["rev-parse", "--verify", "--quiet", "refs/heads/alder"]).is_empty());
    }

    #[tokio::test]
    async fn untracked_work_is_protected_even_if_git_is_set_to_hide_it() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        run(&repo, &["config", "status.showUntrackedFiles", "no"]);
        let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
        let path = Path::new(&prepared.path);
        std::fs::write(path.join("agent-output.txt"), "precious\n").unwrap();

        assert_eq!(status(&prepared.path).await.unwrap().files.len(), 1, "status must show it too");
        let err = remove_workspace(repo.to_str().unwrap(), &prepared.path, false).await.err().unwrap();
        assert_eq!(err.code, ErrorCode::Conflict);
        assert!(path.join("agent-output.txt").exists());
    }

    #[tokio::test]
    async fn commits_only_reachable_from_a_detached_head_are_protected() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
        let path = Path::new(&prepared.path);
        run(path, &["checkout", "-q", "--detach"]);
        std::fs::write(path.join("work.txt"), "work\n").unwrap();
        run(path, &["add", "."]);
        run(path, &["commit", "-qm", "on no branch"]);

        let err = remove_workspace(repo.to_str().unwrap(), &prepared.path, false).await.err().unwrap();
        assert_eq!(err.code, ErrorCode::Conflict, "{}", err.message);
        assert!(path.join("work.txt").exists());
        remove_workspace(repo.to_str().unwrap(), &prepared.path, true).await.unwrap();
    }

    #[tokio::test]
    async fn a_detached_head_on_a_commit_a_branch_holds_can_be_archived() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
        run(Path::new(&prepared.path), &["checkout", "-q", "--detach"]);
        remove_workspace(repo.to_str().unwrap(), &prepared.path, false).await.unwrap();
        assert!(!Path::new(&prepared.path).exists());
    }

    #[tokio::test]
    async fn ignored_files_do_not_block_archiving() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
        let ignored = Path::new(&prepared.path).join("ignored");
        std::fs::create_dir_all(&ignored).unwrap();
        std::fs::write(ignored.join("cache"), "x").unwrap();
        remove_workspace(repo.to_str().unwrap(), &prepared.path, false).await.unwrap();
        assert!(!Path::new(&prepared.path).exists());
    }

    #[tokio::test]
    async fn restoring_brings_back_committed_work() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
        let path = Path::new(&prepared.path);
        std::fs::write(path.join("kept.txt"), "kept\n").unwrap();
        run(path, &["add", "."]);
        run(path, &["commit", "-qm", "work"]);
        let repo_str = repo.to_str().unwrap();
        remove_workspace(repo_str, &prepared.path, false).await.unwrap();

        restore_workspace(repo_str, &prepared.path, "alder").await.unwrap();
        assert_eq!(std::fs::read_to_string(path.join("kept.txt")).unwrap(), "kept\n");
        assert_eq!(restore_workspace(repo_str, &prepared.path, "alder").await.err().unwrap().code, ErrorCode::Conflict);
    }

    #[tokio::test]
    async fn a_worktree_deleted_by_hand_can_still_be_archived_and_restored() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
        std::fs::remove_dir_all(&prepared.path).unwrap();
        let repo_str = repo.to_str().unwrap();
        remove_workspace(repo_str, &prepared.path, false).await.unwrap();
        restore_workspace(repo_str, &prepared.path, "alder").await.unwrap();
        assert!(Path::new(&prepared.path).join("a.txt").exists());
    }

    #[tokio::test]
    async fn status_reports_each_kind_of_change_including_renames_and_odd_names() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
        let path = Path::new(&prepared.path);
        std::fs::write(path.join("a.txt"), "changed\n").unwrap();
        std::fs::write(path.join("with space.txt"), "x\n").unwrap();
        run(path, &["mv", ".gitignore", "renamed"]);

        let status = status(&prepared.path).await.unwrap();
        assert_eq!(status.branch.as_deref(), Some("alder"));
        assert!(status.head.is_some());
        let find =
            |p: &str| status.files.iter().find(|f| f.path == p).unwrap_or_else(|| panic!("{p}: {:?}", status.files));
        assert_eq!(find("a.txt").status, ".M");
        assert_eq!(find("with space.txt").status, "??");
        assert_eq!(find("renamed").orig_path.as_deref(), Some(".gitignore"));
        assert!(find("renamed").status.starts_with('R'));
    }

    #[tokio::test]
    async fn diff_covers_commits_uncommitted_and_untracked_work() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
        let path = Path::new(&prepared.path);
        std::fs::write(path.join("committed.txt"), "from a commit\n").unwrap();
        run(path, &["add", "."]);
        run(path, &["commit", "-qm", "work"]);
        std::fs::write(path.join("a.txt"), "uncommitted\n").unwrap();
        std::fs::write(path.join("untracked.txt"), "brand new\n").unwrap();
        // The base branch moving on must not show up as changes in this workspace.
        std::fs::write(repo.join("moved.txt"), "elsewhere\n").unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "-qm", "main moved"]);

        let diff = diff(&prepared.path, Some("main")).await.unwrap();
        assert!(diff.base_commit.is_some());
        assert!(diff.patch.contains("+from a commit"), "{}", diff.patch);
        assert!(diff.patch.contains("+uncommitted"), "{}", diff.patch);
        assert!(diff.patch.contains("+brand new"), "{}", diff.patch);
        assert!(!diff.patch.contains("elsewhere"), "{}", diff.patch);
        assert!(!diff.truncated);
    }
}
