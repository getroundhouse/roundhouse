//! What git ignores in a worktree, and where the worktree's own git state is, as a watch of its files needs them.

use super::{command, git_error, git_ok, git_ok_bytes, nul_separated};
use process::Environment;
use protocol::client::ApiError;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncWriteExt as _;

/// What git ignores in the worktree at `worktree`, from the top of it, as git writes paths: each topmost folder it
/// ignores whole, ending in `/`, such as `target/` and `node_modules/`, and each file it ignores in a folder it doesn't.
/// A folder holding a file git tracks isn't ignored whole, so its ignored files are listed one by one.
pub(crate) async fn ignored(given: &Environment, worktree: &Path) -> Result<Vec<Vec<u8>>, ApiError> {
    let args = ["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z"];
    let listed = git_ok_bytes(given, worktree, &args).await?;
    Ok(nul_separated(&listed).map(<[u8]>::to_vec).collect())
}

/// Which of `paths`, from the top of the worktree at `worktree`, git ignores, with one git for them all. A path inside a
/// folder git ignores is ignored.
pub(crate) async fn which_ignored(
    given: &Environment,
    worktree: &Path,
    paths: &[Vec<u8>],
) -> Result<Vec<Vec<u8>>, ApiError> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut asking = command(given, worktree, &["check-ignore", "-z", "--stdin"]);
    asking.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = asking.spawn().map_err(|e| git_error(format!("could not run git check-ignore: {e}")))?;
    let mut stdin = child.stdin.take();
    let asked: Vec<u8> = paths.iter().flat_map(|path| path.iter().copied().chain([0])).collect();
    // While git answers, since it can fill what it prints before it has read everything asked.
    let writing = async move {
        if let Some(stdin) = &mut stdin {
            // One git stops reading early fails, which its exit then says.
            let _ = stdin.write_all(&asked).await;
        }
    };
    let ((), output) = tokio::join!(writing, child.wait_with_output());
    let output = output.map_err(|e| git_error(format!("git check-ignore didn't finish: {e}")))?;
    // 1 says none of them is ignored.
    match output.status.code() {
        Some(0 | 1) => Ok(nul_separated(&output.stdout).map(<[u8]>::to_vec).collect()),
        _ => Err(git_error(format!("git check-ignore failed: {}", String::from_utf8_lossy(&output.stderr).trim()))),
    }
}

/// The worktree's own git directory, which holds its index and its `HEAD`: `.git` in a checkout, and the one under the
/// repository's `.git/worktrees/` for a worktree of its own, as a workspace's is.
pub(crate) async fn git_dir(given: &Environment, worktree: &Path) -> Result<PathBuf, ApiError> {
    let dir = git_ok(given, worktree, &["rev-parse", "--absolute-git-dir"]).await?;
    Ok(PathBuf::from(dir.trim_end_matches(['\n', '\r'])))
}
