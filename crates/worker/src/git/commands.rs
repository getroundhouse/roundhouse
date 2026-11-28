use super::{Worktrees, nul_separated};
use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{BranchName, CommitId, GitDiff, GitFile, GitStatus};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use tokio::process::Command;

/// Keeps a diff response bounded when an agent leaves build output untracked and unignored.
const MAX_UNTRACKED_FILES: usize = 200;
const MAX_UNTRACKED_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn git_error(message: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::Worker, message)
}

/// The error for `command`, a git that ended with `status`, which isn't success, having printed `said` on stderr. It
/// says how git ended, with a code or by a signal, since one ended by a signal can have printed nothing.
pub(super) fn git_failed(command: &str, status: ExitStatus, said: &str) -> ApiError {
    let said = said.trim();
    if said.is_empty() {
        return git_error(format!("{command} failed ({status}), and printed nothing"));
    }
    git_error(format!("{command} failed ({status}): {said}"))
}

pub(super) struct GitOutput {
    /// `git` and its arguments, which an error names.
    pub(super) command: String,
    pub(super) status: ExitStatus,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

/// Runs git, with what it prints as it printed it.
pub(super) async fn run(given: &Environment, cwd: &Path, args: &[&str]) -> Result<std::process::Output, ApiError> {
    let output = command(given, cwd, args).output().await;
    output.map_err(|e| git_error(format!("could not run git in {}: {e}", cwd.display())))
}

/// What every git the worker starts is told before its own arguments. On Windows, to read and write paths longer than
/// the 260 characters Git for Windows keeps to unless told, as a build's output in a worktree can have, such as Ghostty's
/// source unpacked under `target/`, or removing a worktree fails part way. As arguments, since the worker gives every
/// process its own git settings through `GIT_CONFIG_COUNT`, and only for the worker's own git, so a person's own
/// settings aren't changed. Git elsewhere has no such limit.
const GIT_FIRST: &[&str] = if cfg!(windows) { &["-c", "core.longpaths=true"] } else { &[] };

/// Git, ready to start, as everything here starts it, with what the worker gives every process it starts.
pub(super) fn command(given: &Environment, cwd: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .args(GIT_FIRST)
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
    given.apply(command.as_std_mut());
    command
}

/// What `git --version` says, such as `git version 2.54.0`.
pub(crate) async fn version(given: &Environment) -> Result<String, ApiError> {
    git_ok(given, Path::new("."), &["--version"]).await
}

/// Where git keeps the programs it runs, as `git --exec-path` says, which is inside the folder git is installed in,
/// such as `C:/Program Files/Git/mingw64/libexec/git-core` for Git for Windows.
#[cfg(windows)]
pub(crate) async fn exec_path(given: &Environment) -> Result<PathBuf, ApiError> {
    let said = git_ok_bytes(given, Path::new("."), &["--exec-path"]).await?;
    Ok(super::path_from_git(said.trim_ascii_end()))
}

pub(super) async fn git(given: &Environment, cwd: &Path, args: &[&str]) -> Result<GitOutput, ApiError> {
    let output = run(given, cwd, args).await?;
    Ok(GitOutput {
        command: format!("git {}", args.join(" ")),
        status: output.status,
        stdout: String::from_utf8_lossy_owned(output.stdout),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

pub(super) async fn git_ok(given: &Environment, cwd: &Path, args: &[&str]) -> Result<String, ApiError> {
    git(given, cwd, args).await?.succeeded()
}

impl GitOutput {
    /// What git printed, if it succeeded, or an error naming the command, with what git said.
    pub(super) fn succeeded(self) -> Result<String, ApiError> {
        if self.status.success() { Ok(self.stdout) } else { Err(git_failed(&self.command, self.status, &self.stderr)) }
    }

    /// What a `git diff --no-index` printed, as [`Self::succeeded`] says, but that it exits with 1 when the two files
    /// differ, which isn't a failure.
    pub(super) fn compared(self) -> Result<String, ApiError> {
        if self.status.code() == Some(1) { Ok(self.stdout) } else { self.succeeded() }
    }
}

/// What git printed, byte for byte, if it succeeded, for output whose paths a person's files can have in bytes that
/// aren't UTF-8, such as a list git separates with NULs.
pub(super) async fn git_ok_bytes(given: &Environment, cwd: &Path, args: &[&str]) -> Result<Vec<u8>, ApiError> {
    let output = run(given, cwd, args).await?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let said = String::from_utf8_lossy(&output.stderr);
    Err(git_failed(&format!("git {}", args.join(" ")), output.status, &said))
}

/// For commands that print NUL-separated paths, such as `git ls-files -z`.
pub(crate) async fn list_nul_separated(
    given: &Environment,
    repo: &Path,
    args: &[String],
) -> Result<Vec<String>, ApiError> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = git_ok_bytes(given, repo, &args).await?;
    Ok(nul_separated(&output).map(|path| String::from_utf8_lossy(path).into_owned()).collect())
}

/// The commit `name` is in the worktree at `cwd`: a branch, a tag, a commit or `HEAD`. `None` if it's none there, as
/// a branch with no commits yet isn't, and an error when git couldn't say, as outside a repository.
pub(super) async fn commit_of(given: &Environment, cwd: &Path, name: &str) -> Result<Option<CommitId>, ApiError> {
    let found = git(given, cwd, &["rev-parse", "--verify", "--quiet", &format!("{name}^{{commit}}")]).await?;
    match found.status.code() {
        Some(0) => Ok(Some(CommitId::from(found.stdout.trim()))),
        // git's word for a name that's no commit.
        Some(1) => Ok(None),
        _ => Err(git_error(format!("git couldn't say what {name} is: {}", found.stderr))),
    }
}

pub(super) async fn ref_exists(given: &Environment, repo: &Path, name: &str) -> Result<bool, ApiError> {
    Ok(commit_of(given, repo, name).await?.is_some())
}

pub(super) fn reject_option_like(value: &str, what: &str) -> Result<(), ApiError> {
    if value.starts_with('-') {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{what} can't start with '-': {value}")));
    }
    Ok(())
}

/// Refuses a branch name git can't have at all, as `InvalidRequest`, before it's looked for, so it's refused as that
/// rather than as one something else has.
pub(super) async fn valid_branch_name(given: &Environment, cwd: &Path, branch: &BranchName) -> Result<(), ApiError> {
    reject_option_like(branch.as_str(), "a branch name")?;
    if !git(given, cwd, &["check-ref-format", "--branch", branch.as_str()]).await?.status.success() {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{branch} isn't a valid branch name")));
    }
    Ok(())
}

/// Filesystem calls block, so they run where that can't hold up other requests.
pub(super) async fn off_thread<T: Send + 'static>(
    path: PathBuf,
    work: impl FnOnce(PathBuf) -> std::io::Result<T> + Send + 'static,
) -> std::io::Result<T> {
    tokio::task::spawn_blocking(move || work(path)).await.map_err(std::io::Error::other)?
}

pub(super) async fn exists(path: &Path) -> bool {
    off_thread(path.to_path_buf(), |path| Ok(path.exists())).await.unwrap_or(false)
}

/// Puts the worktree at `path` back on `branch`, from the repository `repo`, its checkout or its clone.
pub async fn restore_workspace(
    given: &Environment,
    repo: &Path,
    path: &str,
    branch: &BranchName,
) -> Result<(), ApiError> {
    if Path::new(path).exists() {
        return Err(ApiError::new(ErrorCode::Conflict, format!("{path} already exists")));
    }
    if !ref_exists(given, repo, &format!("refs/heads/{branch}")).await? {
        return Err(ApiError::new(ErrorCode::Conflict, format!("branch {branch} no longer exists")));
    }
    let changing = Worktrees::of(given, repo).await?.changing().await?;
    // A worktree deleted outside this app stays registered, and would block re-adding this path.
    changing.prune().await?;
    changing.add(&[path, branch.as_str()]).await?.succeeded()?;
    Ok(())
}

/// Whether a worktree is at `path`: a folder git reads as the top of a working tree. A folder git hasn't made a
/// worktree of yet, or that's something else, isn't. One a `git worktree add` was stopped part way through is, since
/// git reads it as one, with its branch checked out and only some of its files. Fails if git couldn't be run to say, so
/// a hub that settles a record from the answer doesn't take a worktree it couldn't ask about for one that has gone.
pub async fn worktree_there(given: &Environment, path: &str) -> Result<bool, ApiError> {
    let worktree = PathBuf::from(path);
    if !worktree.is_dir() {
        return Ok(false);
    }
    let top = git(given, &worktree, &["rev-parse", "--show-toplevel"]).await?;
    // Git names the top with every link followed, so the path is compared the same way.
    Ok(top.status.success() && std::fs::canonicalize(top.stdout.trim()).ok() == std::fs::canonicalize(&worktree).ok())
}

async fn any_ref_holds_head(given: &Environment, worktree: &Path) -> Result<bool, ApiError> {
    let holders = git_ok(given, worktree, &["for-each-ref", "--contains", "HEAD", "--count=1"]).await?;
    Ok(!holders.trim().is_empty())
}

pub async fn status(given: &Environment, path: &str) -> Result<GitStatus, ApiError> {
    // Explicit, because `status.showUntrackedFiles=no` in someone's config would otherwise hide new
    // files from this, and from the check that keeps archiving from deleting them.
    let args = ["status", "--porcelain=v2", "--branch", "--untracked-files=normal", "-z"];
    let worktree = Path::new(path);
    let output = git_ok(given, worktree, &args).await?;
    let mut status = parse_status(&output);
    status.unreferenced_commits = status.branch.is_none() && !any_ref_holds_head(given, worktree).await?;
    Ok(status)
}

fn parse_status(output: &str) -> GitStatus {
    let mut status = GitStatus::default();
    let mut fields = output.split('\0');
    while let Some(entry) = fields.next() {
        if let Some(header) = entry.strip_prefix("# ") {
            let mut parts = header.split(' ');
            match parts.next() {
                Some("branch.head") => {
                    status.branch = parts.next().filter(|b| *b != "(detached)").map(BranchName::from);
                }
                Some("branch.oid") => status.head = parts.next().filter(|o| *o != "(initial)").map(CommitId::from),
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
pub async fn diff(given: &Environment, worktree_path: &str, base_ref: Option<&str>) -> Result<GitDiff, ApiError> {
    let worktree = Path::new(worktree_path);
    let tracked = async {
        let base = compared_base(given, worktree, base_ref).await?;
        let patch = git_ok(given, worktree, &["diff", "--no-color", "--no-ext-diff", base.as_str()]).await?;
        Ok::<_, ApiError>((base, patch))
    };
    // Beside the diff of what git tracks, since neither changes anything. Its failure is taken first, whichever
    // finishes first.
    let untracked = git_ok_bytes(given, worktree, &["ls-files", "--others", "--exclude-standard", "-z"]);
    let (tracked, untracked) = tokio::join!(tracked, untracked);
    let (base, mut patch) = tracked?;
    let untracked = untracked?;

    let mut truncated = false;
    for (i, file) in nul_separated(&untracked).enumerate() {
        if i >= MAX_UNTRACKED_FILES || patch.len() > MAX_UNTRACKED_BYTES {
            truncated = true;
            break;
        }
        let file = String::from_utf8_lossy(file);
        let args = ["diff", "--no-color", "--no-ext-diff", "--no-index", "--", "/dev/null", &*file];
        patch.push_str(&git(given, worktree, &args).await?.compared()?);
    }
    Ok(GitDiff { base_commit: base, patch, truncated })
}

/// The commit a diff, a change summary, or one file's diff, is compared with, as [`diff_base`] finds it. Fails when HEAD has
/// no commit yet, since there's nothing to compare with.
pub(super) async fn compared_base(
    given: &Environment,
    worktree: &Path,
    base_ref: Option<&str>,
) -> Result<CommitId, ApiError> {
    let base = diff_base(given, worktree, base_ref).await?;
    base.ok_or_else(|| git_error("HEAD has no commit yet, so there's nothing to compare it with"))
}

/// The commit a diff is taken from, by its full object ID, so a client can read a file as it was there: the merge base
/// with `base_ref`, or HEAD's commit when there's no base ref or git finds no merge base with it. `None` only when
/// HEAD has no commit yet.
pub(super) async fn diff_base(
    given: &Environment,
    worktree: &Path,
    base_ref: Option<&str>,
) -> Result<Option<CommitId>, ApiError> {
    if let Some(base) = base_ref {
        let merge_base = git(given, worktree, &["merge-base", base, "HEAD"]).await?;
        if merge_base.status.success() {
            return Ok(Some(CommitId::from(merge_base.stdout.trim())));
        }
    }
    commit_of(given, worktree, "HEAD").await
}
