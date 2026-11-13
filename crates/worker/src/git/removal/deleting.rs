use super::super::{git, git_error, git_ok, off_thread, status};
use crate::deleting::Deleter;
use protocol::client::{ApiError, ErrorCode};
use protocol::worker::RemoveWorkspace;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

/// Beside the worktrees, so moving one in is a rename on the same disk. No workspace's name starts with a dot.
pub(crate) const DELETING: &str = ".deleting";
/// In [`DELETING`], and never deleted, since workers that archive at once share it. See [`Deleting`].
pub(crate) const LOCK: &str = ".lock";

/// How hard git is told to remove a worktree.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Force {
    No,
    Yes,
    /// And past git's own lock on one it was stopped part way through making, which nothing will finish, and git
    /// removes only when told twice. A lock a person put on a worktree is still theirs to take off.
    PartMade,
}

/// Removes the worktree asked, a workspace's own or another made for it, and keeps its branch, so committed work is
/// never lost by archiving. Returns once the worktree's path is free and git has forgotten it, which can be before its
/// files are deleted.
pub async fn remove_workspace(asked: &RemoveWorkspace, deleter: &Deleter) -> Result<(), ApiError> {
    let worktree = Path::new(&asked.path);
    // Another of a workspace's worktrees waits to be deleted where the workspace's own would, beside the workspaces,
    // since every archive in the repository clears what waits there, and nothing clears a folder of `.children/`.
    let beside = asked.own.as_deref().map_or(worktree, Path::new);
    remove(Path::new(&asked.repo_path), worktree, asked.force, &beside.with_file_name(DELETING), deleter).await?;
    if asked.own.is_some()
        && let Some(folder) = worktree.parent()
    {
        // Its workspace's folder of `.children/`, once this was the last in it, which is the only time it goes.
        let _ = off_thread(folder.to_path_buf(), std::fs::remove_dir).await;
    }
    Ok(())
}

async fn remove(repo: &Path, worktree: &Path, force: bool, deleting: &Path, deleter: &Deleter) -> Result<(), ApiError> {
    let path = &*worktree.to_string_lossy();
    if !off_thread(worktree.to_path_buf(), |worktree| Ok(worktree.exists())).await.unwrap_or(true) {
        // Already gone, perhaps deleted by hand, or moved aside by a worker that stopped before git forgot it, which
        // was to be deleted next. Under the lock, so this prune can't forget one moved aside meanwhile.
        // Without it, a worktree moved aside meanwhile is still safe, since `forget` sees git has forgotten it.
        let held = match Deleting::lock(deleting).await {
            Ok(held) => Some(held),
            // The folder it was in has gone too, and with it anything waiting.
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => {
                warn!(path = %deleting.display(), "couldn't lock it, so not deleting what's waiting there: {e}");
                None
            }
        };
        git_ok(repo, &["worktree", "prune"]).await?;
        if let Some(held) = held {
            held.delete_waiting(deleter).await;
        }
        return Ok(());
    }
    // Where git keeps its record of this worktree, which goes when git forgets it.
    let record = git_ok(worktree, &["rev-parse", "--absolute-git-dir"]).await?;
    let record = PathBuf::from(record.trim());
    if !force {
        if let Some(loss) = status(path).await?.loss_if_removed() {
            return Err(ApiError::new(ErrorCode::NeedsForce, loss));
        }
        // Git refuses these unless forced, since a submodule's own commits are in the worktree. Moving it aside would
        // skip that check, so git removes it as it always has.
        if has_submodules(worktree, &record).await? {
            return remove_in_place(repo, path, Force::No).await;
        }
    }
    let force = match force {
        false => Force::No,
        true if locked_part_made(&record).await => Force::PartMade,
        true => Force::Yes,
    };
    // `forget` takes the record being gone to mean git has forgotten the worktree, so it has to be there to begin with.
    if !off_thread(record.clone(), |record| Ok(record.is_dir())).await.unwrap_or(false) {
        warn!(path, record = %record.display(), "git's record of the worktree isn't where git said, so deleting in place");
        return remove_in_place(repo, path, force).await;
    }
    let held = match Deleting::lock(deleting).await {
        Ok(held) => held,
        Err(e) => {
            warn!(path = %deleting.display(), "couldn't delete a worktree in there, so deleting it in place: {e}");
            return remove_in_place(repo, path, force).await;
        }
    };
    let Some(moved) = held.move_in(worktree).await else {
        drop(held);
        return remove_in_place(repo, path, force).await;
    };
    forget(repo, worktree, force, &record, &moved).await?;
    held.delete_waiting(deleter).await;
    Ok(())
}

async fn remove_in_place(repo: &Path, path: &str, force: Force) -> Result<(), ApiError> {
    let mut args = vec!["worktree", "remove"];
    args.extend(match force {
        Force::No => &[][..],
        Force::Yes => &["--force"],
        Force::PartMade => &["--force", "--force"],
    });
    args.push(path);
    git_ok(repo, &args).await?;
    Ok(())
}

/// Removes the worktree at `worktree` if it's one a `git worktree add` was stopped part way through, as when the
/// worker or the machine went, and says whether it was. Git leaves such a one with its branch checked out, only some of
/// its files, and its own lock, and nothing finishes it. Once git has forgotten it, what's left of the directory goes
/// too, since git's own checkout can still be writing there for a moment after git is stopped, which fails git's
/// removal after it has forgotten the worktree. One git won't forget, such as another repository's, is left as it is.
pub(in super::super) async fn remove_part_made(repo: &Path, worktree: &Path) -> Result<bool, ApiError> {
    if !worktree.join(".git").exists() {
        return Ok(false);
    }
    let found = git(worktree, &["rev-parse", "--absolute-git-dir"]).await?;
    let record = PathBuf::from(found.stdout.trim());
    if !found.success || !locked_part_made(&record).await {
        return Ok(false);
    }
    if let Err(refused) = remove_in_place(repo, &worktree.to_string_lossy(), Force::PartMade).await
        && off_thread(record, |record| Ok(record.exists())).await.unwrap_or(true)
    {
        return Err(refused);
    }
    match off_thread(worktree.to_path_buf(), std::fs::remove_dir_all).await {
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            Err(git_error(format!("deleting what was left of {}: {e}", worktree.display())))
        }
        _ => Ok(true),
    }
}

/// Whether git's record of a worktree, at `record`, has the lock git holds while it makes one, which it only leaves
/// behind when it's stopped part way.
async fn locked_part_made(record: &Path) -> bool {
    let reason = off_thread(record.join("locked"), std::fs::read_to_string).await;
    reason.is_ok_and(|reason| reason.trim() == "initializing")
}

/// Whether git might find submodules in the worktree: a gitlink in its index, or a submodule's repository in its git
/// directory, `record`. Git looks more closely, and removing in place lets it.
async fn has_submodules(worktree: &Path, record: &Path) -> Result<bool, ApiError> {
    if off_thread(record.join("modules"), |modules| Ok(modules.is_dir())).await.unwrap_or(true) {
        return Ok(true);
    }
    let index = git_ok(worktree, &["ls-files", "--stage", "-z"]).await?;
    Ok(index.split('\0').any(|entry| entry.starts_with("160000 ")))
}

/// Has git forget the worktree moved aside to `moved`. With its directory gone, git removes only its own record of
/// it. If git refuses, such as for a locked worktree, it goes back, so a refusal changes nothing. Git may have
/// forgotten it already, since a `git worktree prune` forgets a worktree whose directory is missing, as restoring a
/// workspace beside this one does, and `git gc` does too. Then there's nothing to put back.
pub(super) async fn forget(
    repo: &Path,
    worktree: &Path,
    force: Force,
    record: &Path,
    moved: &Path,
) -> Result<(), ApiError> {
    let path = worktree.to_string_lossy();
    let Err(refused) = remove_in_place(repo, &path, force).await else { return Ok(()) };
    if !off_thread(record.to_path_buf(), |record| Ok(record.exists())).await.unwrap_or(true) {
        return Ok(());
    }
    let Err(e) = rename(moved, worktree).await else { return Err(refused) };
    // Something made the path again meanwhile. Kept beside it, where nothing deletes it.
    let kept = worktree.with_file_name(format!(".{}-kept-{}", file_name(worktree), stamp()));
    match rename(moved, &kept).await {
        Ok(()) => Err(git_error(format!(
            "{}, and it couldn't go back to {path} ({e}), so it's in {}",
            refused.message,
            kept.display()
        ))),
        Err(also) => {
            error!(path = %moved.display(), "a worktree git refused to remove couldn't be put back ({e}) or kept: {also}");
            Err(git_error(format!("{}, and it couldn't go back to {path} ({e})", refused.message)))
        }
    }
}

/// A [`DELETING`], locked from moving a worktree in until git has forgotten it or it has gone back, and while what's
/// there is listed to be deleted. So a worktree that may yet go back is never listed, even by another worker
/// archiving beside it at the same time.
struct Deleting {
    pub(super) dir: PathBuf,
    pub(super) _lock: File,
}

impl Deleting {
    /// Waits for the lock, which another archive holds only for a rename and a quick git command. Makes the directory
    /// if it isn't there, but not the one it's in.
    pub(super) async fn lock(dir: &Path) -> io::Result<Self> {
        let lock = off_thread(dir.to_path_buf(), |dir| {
            match std::fs::create_dir(&dir) {
                Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e),
                _ => {}
            }
            let file = OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(LOCK))?;
            file.lock().map(|()| file)
        })
        .await?;
        Ok(Self { dir: dir.to_path_buf(), _lock: lock })
    }

    /// Moves the worktree in, under a name nothing else there has. `None` if it can't go there, such as when the
    /// worktree is a disk of its own.
    pub(super) async fn move_in(&self, worktree: &Path) -> Option<PathBuf> {
        let moved = self.dir.join(format!("{}-{}", file_name(worktree), stamp()));
        match rename(worktree, &moved).await {
            Ok(()) => Some(moved),
            Err(e) => {
                warn!(path = %worktree.display(), "couldn't move the worktree aside, so deleting it in place: {e}");
                None
            }
        }
    }

    /// Deletes everything waiting, including what an earlier archive left, such as one a shutdown cut short. What
    /// can't be deleted stays, to be tried again by the next archive beside it.
    pub(super) async fn delete_waiting(self, deleter: &Deleter) {
        let listed = off_thread(self.dir.clone(), |dir| {
            let mut waiting = Vec::new();
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                if entry.file_name() != LOCK {
                    waiting.push(entry.path());
                }
            }
            Ok(waiting)
        })
        .await;
        drop(self);
        match listed {
            Ok(waiting) if waiting.is_empty() => {}
            Ok(waiting) => {
                info!(count = waiting.len(), "deleting worktrees moved aside");
                deleter.start(&waiting);
            }
            Err(e) => warn!("couldn't list worktrees moved aside to delete them: {e}"),
        }
    }
}

/// When it was moved aside, which keeps names apart.
fn stamp() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_nanos())
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_default(|name| name.to_string_lossy().into_owned())
}

async fn rename(from: &Path, to: &Path) -> io::Result<()> {
    let to = to.to_path_buf();
    off_thread(from.to_path_buf(), move |from| std::fs::rename(from, to)).await
}
