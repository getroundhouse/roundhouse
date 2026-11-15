use super::super::{GitOutput, git, git_error, git_ok, off_thread};
use protocol::client::ApiError;
use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use tracing::info;

/// The lock's file, in the repository's git directory. Never deleted, since workers share it. Not named as git names
/// its own short-lived `.lock` files, which people are told they can delete when one is left behind. *Unconfirmed*
const LOCK: &str = "roundhouse-worktrees";

/// What a worker logs when it has to wait for the lock, since that's why making or removing a worktree can take a
/// while.
pub(crate) const WAITING: &str = "waiting for git elsewhere to be done with the repository's worktrees";

/// A repository's worktrees, which a worker changes only while it holds their lock alone, with [`Worktrees::changing`],
/// and reads every one of only while it holds it beside others reading, with [`Worktrees::reading`]. What each holds
/// is the only way to run the commands it's for.
///
/// It's taken after any other lock a worker holds as it changes a worktree, such as `Deleting`'s, and never before
/// one. And it's never asked for while it's held, since the second ask would wait on the first, even in one process.
pub(in crate::git) struct Worktrees {
    /// Where git runs: the repository's own checkout, or one of its worktrees.
    cwd: PathBuf,
    pub(super) lock: PathBuf,
}

/// A repository's worktrees, held alone.
pub(in crate::git) struct Changing {
    cwd: PathBuf,
    _file: File,
}

/// A repository's worktrees, held beside others reading them, so nothing changes them meanwhile.
pub(in crate::git) struct Reading {
    cwd: PathBuf,
    _file: File,
}

#[derive(Clone, Copy)]
pub(super) enum Hold {
    /// For a command that changes the worktrees.
    Alone,
    /// For a command that only reads them.
    BesideOthersReading,
}

impl Worktrees {
    /// The worktrees of the repository that `cwd` is in, whether its own checkout or one of its worktrees.
    pub(in crate::git) async fn of(cwd: &Path) -> Result<Self, ApiError> {
        let dir = git_ok(cwd, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).await?;
        Ok(Self { cwd: cwd.to_path_buf(), lock: PathBuf::from(dir.trim()).join(LOCK) })
    }

    /// Waits until nothing else holds them.
    pub(in crate::git) async fn changing(&self) -> Result<Changing, ApiError> {
        Ok(Changing { cwd: self.cwd.clone(), _file: self.hold(Hold::Alone).await? })
    }

    /// Waits until nothing that changes them holds them.
    pub(in crate::git) async fn reading(&self) -> Result<Reading, ApiError> {
        Ok(Reading { cwd: self.cwd.clone(), _file: self.hold(Hold::BesideOthersReading).await? })
    }

    async fn hold(&self, how: Hold) -> Result<File, ApiError> {
        let failed =
            |e: io::Error| git_error(format!("couldn't lock the repository's worktrees, {}: {e}", self.lock.display()));
        let file = off_thread(self.lock.clone(), |path| open(&path)).await.map_err(failed)?;
        let tried = match how {
            Hold::Alone => file.try_lock(),
            Hold::BesideOthersReading => file.try_lock_shared(),
        };
        match tried {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) => info!(lock = %self.lock.display(), "{WAITING}"),
            Err(TryLockError::Error(e)) => return Err(failed(e)),
        }
        // Off the runtime's threads, since it blocks for as long as git elsewhere takes.
        off_thread(self.lock.clone(), move |_| {
            match how {
                Hold::Alone => file.lock(),
                Hold::BesideOthersReading => file.lock_shared(),
            }
            .map(|()| file)
        })
        .await
        .map_err(failed)
    }
}

/// Opens the lock's file, making it if it isn't there. Only to read, if this user can't write it, as in a repository
/// shared with someone whose worker made it, since holding the lock needs only the file open, on a local disk at
/// least. If that fails too, the refusal to write is what says why.
fn open(path: &Path) -> io::Result<File> {
    match OpenOptions::new().create(true).truncate(false).write(true).open(path) {
        Err(refused) if refused.kind() == io::ErrorKind::PermissionDenied => File::open(path).map_err(|_| refused),
        opened => opened,
    }
}

impl Changing {
    /// Has git forget the worktrees whose directories have gone, as by an agent's `rm -rf`, which stay registered and
    /// would refuse their paths and their branches.
    pub(in crate::git) async fn prune(&self) -> Result<(), ApiError> {
        git_ok(&self.cwd, &["worktree", "prune"]).await.map(drop)
    }

    /// Runs `git worktree add` with `args`.
    pub(in crate::git) async fn add(&self, args: &[&str]) -> Result<GitOutput, ApiError> {
        git(&self.cwd, &[&["worktree", "add"], args].concat()).await
    }

    /// Runs `git worktree remove` with `args`.
    pub(in crate::git) async fn remove(&self, args: &[&str]) -> Result<(), ApiError> {
        git_ok(&self.cwd, &[&["worktree", "remove"], args].concat()).await.map(drop)
    }
}

impl Reading {
    /// Runs `git fetch` with `args`.
    pub(in crate::git) async fn fetch(&self, args: &[&str]) -> Result<GitOutput, ApiError> {
        git(&self.cwd, &[&["fetch"], args].concat()).await
    }
}
