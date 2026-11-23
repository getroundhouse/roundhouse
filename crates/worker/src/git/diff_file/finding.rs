//! How git has a file changed against the base, which says which of git's diffs is the file's.

use super::super::changes::{tracked, untracked};
use super::super::git;
use process::Environment;
use protocol::client::ApiError;
use protocol::model::{ChangeStatus, CommitId};
use std::path::Path;

/// What an untracked file is compared with, which `git diff --no-index` reads as no file on every platform.
pub(super) const NOTHING: &str = "/dev/null";

/// What git finds of the file asked about.
pub(super) enum Found {
    /// A change with lines, whose patch is git's diff of these paths.
    Patch(Patch),
    /// A change git counts no lines of, since it takes the file as binary or it's larger than git reads to compare.
    Binary,
    /// No change: the file is as the base has it, or it's in neither, or git ignores it.
    Unchanged,
}

/// Which of git's diffs is the file's patch.
pub(super) enum Patch {
    /// Of a file git tracks, against the base, at these paths: where a rename moved it from and then its own, or its
    /// own alone.
    Tracked(Vec<String>),
    /// Of a file git neither tracks nor ignores, against [`NOTHING`].
    Untracked(String),
}

/// How git has `file` of the worktree at `worktree` changed against `base`, where `from` is where the summary says a
/// rename moved it from. If git no longer finds that rename, it's what git now says of `file` alone.
pub(super) async fn found(
    given: &Environment,
    worktree: &Path,
    base: &CommitId,
    file: &str,
    from: Option<&str>,
) -> Result<Found, ApiError> {
    let paths: Vec<&str> = from.into_iter().chain([file]).collect();
    let tracked = tracked(given, worktree, base, &paths).await?;
    if let Some(listed) = tracked.into_iter().find(|listed| listed.path == file.as_bytes()) {
        if listed.lines.is_none() {
            return Ok(Found::Binary);
        }
        let renamed = listed.status == ChangeStatus::Renamed && listed.from.as_deref() == from.map(str::as_bytes);
        let paths = if renamed { paths } else { vec![file] };
        return Ok(Found::Patch(Patch::Tracked(paths.into_iter().map(str::to_string).collect())));
    }
    let untracked = untracked(given, worktree, &[file]).await?;
    if !untracked.iter().any(|listed| listed.path == file.as_bytes()) {
        return Ok(Found::Unchanged);
    }
    // git counts no lines of an untracked file but against nothing, as its patch is taken.
    let args = ["diff", "--no-index", "--numstat", "--no-ext-diff", "--no-textconv", "--", NOTHING, file];
    let counted = git(given, worktree, &args).await?.compared()?;
    if counted.starts_with("-\t-\t") {
        return Ok(Found::Binary);
    }
    Ok(Found::Patch(Patch::Untracked(file.to_string())))
}
