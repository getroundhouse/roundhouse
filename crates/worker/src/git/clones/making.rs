use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use process::Environment;
use protocol::client::ApiError;
use protocol::worker::RepoSource;
use tracing::{info, warn};

use super::super::{Hold, exists, git, git_error, git_ok, held, off_thread, reject_option_like};
use super::Clones;
use crate::deleting::delete_trees;

/// Long enough to clone a repository of a few gigabytes over a connection of a few megabytes a second, and short
/// enough that a remote that never answers doesn't keep its repository's first workspace, and every worker waiting its
/// turn behind it, from being made or refused for good. *Unconfirmed*
const CLONE_TIMEOUT: Duration = Duration::from_mins(30);

/// What a worker logs when it waits for another to finish cloning a repository, since that's why making a workspace
/// can take a while.
pub(super) const WAITING: &str = "waiting for another worker to finish cloning the repository";

impl Clones {
    /// Where git runs for `source`'s repository, as [`Clones::repository`] says, once there's a clone of an address,
    /// made first if there's none: by this worker, or by another that's making it already, whose turn this waits for.
    pub(crate) async fn made_for(&self, given: &Environment, source: &RepoSource) -> Result<PathBuf, ApiError> {
        match source {
            RepoSource::Checkout { path } => Ok(PathBuf::from(path)),
            RepoSource::Clone { url } => self.made(given, url).await,
        }
    }

    async fn made(&self, given: &Environment, url: &str) -> Result<PathBuf, ApiError> {
        reject_option_like(url, "a repository's address")?;
        let whole = self.of(url);
        if exists(&whole).await {
            return Ok(whole);
        }
        let (making, lock) = self.making(url);
        let _turn = turn(&lock).await?;
        if exists(&whole).await {
            // Made by the worker whose turn this waited for.
            return Ok(whole);
        }
        // What a worker that was stopped as it made it left there.
        cleared(&making).await?;
        info!(url, "cloning the repository");
        let placed = match tokio::time::timeout(CLONE_TIMEOUT, clone(given, url, &making)).await {
            Ok(Ok(())) => rename(&making, &whole).await,
            Ok(Err(e)) => Err(e),
            Err(_) => {
                Err(git_error(format!("cloning {url} took longer than {} minutes", CLONE_TIMEOUT.as_secs() / 60)))
            }
        };
        if placed.is_err()
            && let Err(e) = cleared(&making).await
        {
            // The next worker to make it clears it first, as it does what a stop left.
            warn!(url, "couldn't clear the clone that failed: {}", e.message);
        }
        placed.map(|()| whole)
    }
}

/// The turn at making a clone, held alone, by its lock's file at `lock`, once no other worker on the machine holds it.
async fn turn(lock: &Path) -> Result<File, ApiError> {
    let failed = |e: io::Error| git_error(format!("couldn't take a turn at cloning, {}: {e}", lock.display()));
    let folder = lock.parent().map_or_else(PathBuf::new, Path::to_path_buf);
    off_thread(folder, std::fs::create_dir_all).await.map_err(failed)?;
    held(lock, Hold::Alone, WAITING).await.map_err(failed)
}

/// Makes a bare clone of `url` at `making`, as `git clone` sets one up but for a branch and files of its own: with
/// `origin`'s branches and tags, and `origin`'s `HEAD`, which a workspace is made from unless it's asked for another
/// base. Every git it runs is given `given`, which holds the git credential the worker's hub set.
async fn clone(given: &Environment, url: &str, making: &Path) -> Result<(), ApiError> {
    let folder = making.parent().unwrap_or(making);
    git_ok(given, folder, &["init", "--quiet", "--bare", &making.to_string_lossy()]).await?;
    git_ok(given, making, &["remote", "add", "origin", url]).await?;
    git_ok(given, making, &["fetch", "--quiet", "origin"]).await?;
    let head = git(given, making, &["remote", "set-head", "origin", "--auto"]).await?;
    if !head.success {
        // A workspace's base is `origin`'s `HEAD` only by default, and then `origin/main` or `origin/master`, so a clone
        // of a remote with none, as an empty one is, is whole without it.
        warn!(url, "cloned the repository without origin's HEAD: {}", head.stderr);
    }
    Ok(())
}

/// Deletes what's at `making`, if anything is.
async fn cleared(making: &Path) -> Result<(), ApiError> {
    let deleted =
        off_thread(making.to_path_buf(), |making| delete_trees(&[making]).pop().map_or(Ok(()), |(_, e)| Err(e)));
    deleted.await.map_err(|e| git_error(format!("couldn't clear {}: {e}", making.display())))
}

async fn rename(from: &Path, to: &Path) -> Result<(), ApiError> {
    let failed = |e: io::Error| git_error(format!("couldn't move the clone into {}: {e}", to.display()));
    let to = to.to_path_buf();
    off_thread(from.to_path_buf(), move |from| std::fs::rename(from, to)).await.map_err(failed)
}
