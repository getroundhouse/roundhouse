//! What the repo's history says: where the checkout is, its commit, and what changed since another.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, bail};

use crate::program;

/// The checkout this is run in, which holds the app's Xcode project and the history its version counts.
pub(crate) fn root() -> anyhow::Result<PathBuf> {
    let top = program::output(Command::new("git").args(["rev-parse", "--show-toplevel"]))
        .context("run this in a checkout of the repo")?;
    Ok(PathBuf::from(top.trim()))
}

pub(crate) fn head(root: &Path) -> anyhow::Result<String> {
    Ok(program::output(git(root).args(["rev-parse", "HEAD"]))?.trim().to_string())
}

/// Whether any of `paths` differs between `commit` and HEAD.
pub(crate) fn differs(root: &Path, commit: &str, paths: &[String]) -> anyhow::Result<bool> {
    let status = git(root)
        .args(["diff", "--quiet", commit, "HEAD", "--"])
        .args(paths)
        .status()
        .context("couldn't run `git diff`")?;
    // `--quiet` exits 1 for a difference, and anything else for an error, such as a commit it doesn't have.
    match status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => bail!("`git diff` from {commit} failed, with {status}"),
    }
}

/// The titles of the commits since `commit` that changed any of `paths`, newest first.
pub(crate) fn titles(root: &Path, commit: &str, paths: &[String]) -> anyhow::Result<Vec<String>> {
    let log = program::output(git(root).args(["log", "--format=%s", &format!("{commit}..HEAD"), "--"]).args(paths))?;
    Ok(log.lines().map(str::to_string).collect())
}

fn git(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(root);
    command
}
