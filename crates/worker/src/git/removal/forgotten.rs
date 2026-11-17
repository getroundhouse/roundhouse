//! A worktree's folder that git has forgotten. `git worktree remove` forgets a worktree even when it couldn't delete
//! all of its folder, as when a file in it is held open or its path is too long, and leaves what's left with a `.git`
//! file that names a record that's gone. Git can't say what's in it, so it's deleted only when forced.

use super::super::{git_ok, off_thread};
use super::deleting::{Deleting, couldnt_lock};
use crate::deleting::Deleter;
use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use std::path::{Path, PathBuf};

/// Whether `worktree` is one of `repo`'s worktrees that git has forgotten: its `.git` file names a record in the
/// repository's `worktrees/` that isn't there. A folder of another repository's, or no worktree's, isn't.
pub(super) async fn forgotten(given: &Environment, repo: &Path, worktree: &Path) -> Result<bool, ApiError> {
    let common = git_ok(given, repo, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).await?;
    let common = PathBuf::from(common.trim());
    let named = off_thread(worktree.join(".git"), move |dot_git| Ok(names_a_record_gone(&dot_git, &common))).await;
    Ok(named.unwrap_or(false))
}

/// Whether the `.git` file at `dot_git` names a record in the `worktrees/` of the repository whose git directory is
/// `common` that isn't there. Git writes the record's path absolute, or relative to the file.
fn names_a_record_gone(dot_git: &Path, common: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(dot_git) else { return false };
    let Some(named) = text.trim_end().strip_prefix("gitdir: ") else { return false };
    let record = dot_git.parent().map_or_else(|| PathBuf::from(named), |folder| folder.join(named));
    let Some(list) = record.parent() else { return false };
    if list.file_name().is_none_or(|name| name != "worktrees") || record.exists() {
        return false;
    }
    let same = |a: &Path, b: &Path| matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b);
    list.parent().is_some_and(|git_dir| same(git_dir, common))
}

/// Deletes the folder of a worktree git has forgotten, as one moved aside is deleted, once forced. Refuses with
/// `NeedsForce` until then, since git can't say whether what's left holds work. Its branch is kept, as always.
pub(super) async fn remove_forgotten(
    given: &Environment,
    worktree: &Path,
    force: bool,
    deleting: &Path,
    deleter: &Deleter,
) -> Result<(), ApiError> {
    if !force {
        return Err(ApiError::new(
            ErrorCode::NeedsForce,
            "git has already forgotten this worktree, so what's left of its folder can't be checked for work, and \
             archiving deletes it",
        ));
    }
    let held = Deleting::lock(deleting).await.map_err(|e| couldnt_lock(deleting, &e))?;
    held.move_in(worktree).await?;
    held.delete_waiting(given, deleter).await;
    Ok(())
}
