//! Taking one page of one file's diff, with the version of the file it's of.

use super::super::changes::version_now;
use super::super::{command, compared_base, off_thread};
use super::finding::{Found, NOTHING, Patch, found};
use super::pages::patch_page;
use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{CommitId, FileDiff, FileDiffBody, FileVersion, MOST_PATCH_PAGE_BYTES};
use protocol::worker::DiffFile;
use std::path::{Component, Path, PathBuf};

/// What git's diff is told, whatever a person's config says: no program of theirs runs, a patch has no colours and
/// three lines of context, and a path with `*` or `:` in it names only itself.
const DIFF: &[&str] = &["--literal-pathspecs", "diff", "--no-ext-diff", "--no-textconv", "--no-color", "--unified=3"];

/// The page `asked` names of the diff of one file of a worktree, against the base its change summary is taken from.
pub(crate) async fn diff_file(given: &Environment, asked: &DiffFile) -> Result<FileDiff, ApiError> {
    let worktree = Path::new(&asked.path);
    let named = inside(&asked.file)?;
    if let Some(from) = &asked.from {
        inside(from)?;
    }
    let base = compared_base(given, worktree, asked.base_ref.as_deref()).await?;
    let (version, body) = of_one_version(worktree, named, body(given, worktree, &base, asked)).await?;
    Ok(FileDiff { base_commit: base, version, body })
}

/// What `reading` gives, with the version of the file at `path` of the worktree at `worktree` while it ran, which is
/// read before and after, and is `None` if the two differ.
pub(super) async fn of_one_version<T>(
    worktree: &Path,
    path: &Path,
    reading: impl Future<Output = Result<T, ApiError>>,
) -> Result<(Option<FileVersion>, T), ApiError> {
    let before = version(worktree, path).await?;
    let read = reading.await?;
    let after = version(worktree, path).await?;
    Ok((before.filter(|before| after.as_ref() == Some(before)), read))
}

async fn body(
    given: &Environment,
    worktree: &Path,
    base: &CommitId,
    asked: &DiffFile,
) -> Result<FileDiffBody, ApiError> {
    let patch = match found(given, worktree, base, &asked.file, asked.from.as_deref()).await? {
        Found::Patch(patch) => patch,
        Found::Binary => return Ok(FileDiffBody::Binary),
        Found::Unchanged => return Ok(FileDiffBody::Unchanged),
    };
    let (args, compared) = match &patch {
        Patch::Tracked(paths) => {
            let paths = paths.iter().map(String::as_str);
            ([DIFF, &["--find-renames", base.as_str(), "--"]].concat().into_iter().chain(paths).collect(), false)
        }
        Patch::Untracked(file) => ([DIFF, &["--no-index", "--", NOTHING, file]].concat(), true),
    };
    let page = patch_page(command(given, worktree, &args), asked.first_line, MOST_PATCH_PAGE_BYTES, compared).await?;
    Ok(FileDiffBody::Patch { patch: page.patch, next_line: page.next_line })
}

/// `path` as a path from the top of the worktree, which is how git and a summary name a file. Refused if it's empty,
/// absolute, or goes up a folder, since then it names nothing in the worktree.
fn inside(path: &str) -> Result<&Path, ApiError> {
    let named = Path::new(path);
    let mut parts = named.components().peekable();
    if parts.peek().is_some() && parts.all(|part| matches!(part, Component::Normal(_))) {
        return Ok(named);
    }
    Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} isn't a path inside the worktree")))
}

/// As [`version_now`] reads it, off the async threads, since it reads the whole file.
async fn version(worktree: &Path, path: &Path) -> Result<Option<FileVersion>, ApiError> {
    let path = path.to_path_buf();
    let read = off_thread(worktree.to_path_buf(), move |worktree: PathBuf| Ok(version_now(&worktree, &path))).await;
    read.map_err(|e| ApiError::new(ErrorCode::Internal, format!("reading a file's version stopped: {e}")))
}
