//! Which files a worktree changes, as git lists them: those it tracks with their lines, from one diff against the base
//! commit, and those it doesn't track or ignore.

use super::super::{compared_base, git_error, git_ok_bytes, nul_separated};
use process::Environment;
use protocol::client::ApiError;
use protocol::model::{ChangeStatus, CommitId, LineCounts};
use std::path::Path;

/// A file git lists as changed, before the worker reads what the worktree holds of it.
pub(in crate::git) struct Listed {
    /// As git wrote it, which is how the worker finds it in the worktree.
    pub(in crate::git) path: Vec<u8>,
    pub(in crate::git) from: Option<Vec<u8>>,
    pub(in crate::git) status: ChangeStatus,
    /// git's count. `None` for a file git counts no lines of, and for an untracked one, which git doesn't count.
    pub(in crate::git) lines: Option<LineCounts>,
}

/// The commit the worktree at `worktree` is compared with, as [`compared_base`] finds it from `base_ref`, and each
/// file it changes: the tracked ones in git's order, then the untracked ones.
pub(super) async fn listed(
    given: &Environment,
    worktree: &Path,
    base_ref: Option<&str>,
) -> Result<(CommitId, Vec<Listed>), ApiError> {
    let tracked = async {
        let base = compared_base(given, worktree, base_ref).await?;
        let files = tracked(given, worktree, &base, &[]).await?;
        Ok::<_, ApiError>((base, files))
    };
    // Beside the diff, since neither changes anything. Its failure is taken first, whichever finishes first.
    let (tracked, untracked) = tokio::join!(tracked, untracked(given, worktree, &[]));
    let (base, mut files) = tracked?;
    files.extend(untracked?);
    Ok((base, files))
}

/// Each file git tracks that the worktree at `worktree` changes against `base`, in git's order: those at `paths`, as
/// they're written, or every one where `paths` is empty.
pub(in crate::git) async fn tracked(
    given: &Environment,
    worktree: &Path,
    base: &CommitId,
    paths: &[&str],
) -> Result<Vec<Listed>, ApiError> {
    // Renames are looked for whatever `diff.renames` says, and copies aren't, so a copy is an added file with all its
    // lines added. No program a person's config names runs, so lines are counted from what the files hold. A path
    // with `*` or `:` in it names only itself.
    let args = [
        "--literal-pathspecs",
        "diff",
        "--raw",
        "--numstat",
        "-z",
        "--find-renames",
        "--no-ext-diff",
        "--no-textconv",
        base.as_str(),
        "--",
    ];
    let output = git_ok_bytes(given, worktree, &[&args[..], paths].concat()).await?;
    tracked_files(&output)
}

/// Each file at `paths` in the worktree at `worktree`, or every one where `paths` is empty, that git neither tracks
/// nor ignores, which git counts no lines of.
pub(in crate::git) async fn untracked(
    given: &Environment,
    worktree: &Path,
    paths: &[&str],
) -> Result<Vec<Listed>, ApiError> {
    let args = ["--literal-pathspecs", "ls-files", "--others", "--exclude-standard", "-z", "--"];
    let output = git_ok_bytes(given, worktree, &[&args[..], paths].concat()).await?;
    Ok(nul_separated(&output)
        .map(|path| Listed { path: path.to_vec(), from: None, status: ChangeStatus::Untracked, lines: None })
        .collect())
}

/// What `git diff --raw --numstat -z` printed: every file's raw entry, `:` and its modes, objects and status, then its
/// path, or for a rename the path it had and the one it has. Then every file's counts, in the same order, as the lines
/// it adds and removes and its path, or for a rename nothing, then its two paths.
fn tracked_files(output: &[u8]) -> Result<Vec<Listed>, ApiError> {
    let mut fields = output.split(|b| *b == 0);
    let (mut files, mut counts) = (Vec::new(), Vec::new());
    while let Some(field) = fields.next() {
        if field.is_empty() {
            // After the last NUL.
            continue;
        }
        if let Some(raw) = field.strip_prefix(b":") {
            let letter = raw.rsplit(|b| *b == b' ').next().and_then(|status| status.first()).ok_or_else(unexpected)?;
            let status = status_of(*letter);
            let first = fields.next().ok_or_else(unexpected)?.to_vec();
            let (from, path) = if status == ChangeStatus::Renamed {
                (Some(first), fields.next().ok_or_else(unexpected)?.to_vec())
            } else {
                (None, first)
            };
            files.push(Listed { path, from, status, lines: None });
        } else {
            let mut parts = field.splitn(3, |b| *b == b'\t');
            let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
                return Err(unexpected());
            };
            if path.is_empty() {
                // A rename's, whose paths its raw entry has already given.
                fields.next();
                fields.next();
            }
            counts.push(lines(added, removed)?);
        }
    }
    if files.len() != counts.len() {
        return Err(unexpected());
    }
    for (file, lines) in files.iter_mut().zip(counts) {
        file.lines = lines;
    }
    Ok(files)
}

/// git's letter for how a file changed, from its raw entry.
fn status_of(letter: u8) -> ChangeStatus {
    match letter {
        b'A' => ChangeStatus::Added,
        b'D' => ChangeStatus::Deleted,
        b'R' => ChangeStatus::Renamed,
        b'T' => ChangeStatus::TypeChanged,
        // `M`, and every other, none of which this diff gives: `C`, since copies aren't looked for, `U`, which a
        // diff against a commit gives a file in conflict as `M`, and `X`, which is a fault in git. Whatever it is, the
        // file is changed.
        _ => ChangeStatus::Modified,
    }
}

/// `None` for a file git counts no lines of, which it says with `-` for each.
fn lines(added: &[u8], removed: &[u8]) -> Result<Option<LineCounts>, ApiError> {
    if added == b"-" && removed == b"-" {
        return Ok(None);
    }
    let count = |n: &[u8]| std::str::from_utf8(n).ok().and_then(|n| n.parse().ok()).ok_or_else(unexpected);
    Ok(Some(LineCounts { added: count(added)?, removed: count(removed)? }))
}

fn unexpected() -> ApiError {
    git_error("git's diff listed its files in a form the worker doesn't read")
}
