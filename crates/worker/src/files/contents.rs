use crate::git::files::{Tracked, entries};
use protocol::attachment::{AttachmentBytes, MediaType};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{CommitId, FileContents, WorktreeFiles};
use std::collections::HashSet;
use std::io;
use std::io::Read as _;
use std::path::{Component, Path};

/// Far more than a repository an agent works in usually has, and few enough that a client lays them out in a moment.
const MAX_FILES: usize = 50_000;
/// About a hundred thousand lines, more than anyone reads in a viewer, and small enough to send at once.
pub(super) const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;
/// Far bigger than a screenshot or a photo, since an image can't be shown in part, and still a third of a frame once
/// it's base64, as [`protocol::frame::MAX_FRAME_BYTES`] caps it.
pub(super) const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;
/// How far into a file git looks for a NUL byte, which says it isn't text.
pub(super) const SNIFF_BYTES: usize = 8000;

/// The files in the worktree at `worktree`: those git tracks, apart from any deleted since, new ones, and ignored ones.
/// A folder git ignores whole, or that's a repository of its own, whether a submodule or one git doesn't know, is
/// listed as a folder rather than by its files, since one such as `target` can hold hundreds of thousands, and so is a
/// link that leads to a folder.
pub(crate) async fn list(worktree: &Path) -> Result<WorktreeFiles, ApiError> {
    let entries = entries(worktree).await?;
    let (mut tracked, mut folders) = (Vec::new(), Vec::new());
    for (path, kind) in entries.tracked.into_iter().filter(|(path, _)| !entries.deleted.contains(path)) {
        match kind {
            Tracked::Submodule => folders.push(path),
            Tracked::Link if worktree.join(&path).is_dir() => folders.push(path),
            Tracked::File | Tracked::Link => tracked.push(path),
        }
    }
    let (untracked, untracked_folders) = split_folders(entries.untracked);
    let untracked = links_to_folders(worktree, untracked, &mut folders);
    folders.extend(untracked_folders);
    let (ignored, ignored_folders) = split_folders(entries.ignored);
    let (ignored, mut ignored_folders) = outside(ignored, ignored_folders);
    let ignored = links_to_folders(worktree, ignored, &mut ignored_folders);
    let (paths, truncated) = bounded([tracked, untracked, ignored], MAX_FILES);
    folders.extend(ignored_folders);
    folders.sort_unstable();
    folders.dedup();
    Ok(WorktreeFiles { paths, folders, truncated })
}

/// Leaves out what's inside a folder that's listed whole, which git also lists when a pattern for files ignores a
/// folder's every file, as `*.log` does `logs/`: the folder, and then each file in it.
fn outside(mut files: Vec<String>, mut folders: Vec<String>) -> (Vec<String>, Vec<String>) {
    let whole: HashSet<String> = folders.iter().cloned().collect();
    let outside = |path: &String| !path.match_indices('/').any(|(at, _)| whole.contains(&path[..at]));
    files.retain(outside);
    folders.retain(outside);
    (files, folders)
}

/// The files git lists that are files, with each link that leads to a folder put in `folders` instead, since git lists
/// a link as a file whatever it leads to, and it opens as a folder.
fn links_to_folders(worktree: &Path, files: Vec<String>, folders: &mut Vec<String>) -> Vec<String> {
    let (links, files): (Vec<String>, Vec<String>) = files.into_iter().partition(|path| worktree.join(path).is_dir());
    folders.extend(links);
    files
}

/// What git lists, as files and folders, which git ends with a slash.
fn split_folders(entries: impl IntoIterator<Item = String>) -> (Vec<String>, Vec<String>) {
    let (mut files, mut folders) = (Vec::new(), Vec::new());
    for entry in entries {
        match entry.strip_suffix('/') {
            Some(folder) => folders.push(folder.to_string()),
            None => files.push(entry),
        }
    }
    (files, folders)
}

/// At most `max` of the files, in order, and whether any were left out. Each of `kinds` gives way to those before it,
/// so ignored files are left out first and then new ones, and no file git tracks goes missing to make room for build
/// output, or for one that isn't ignored, such as a `node_modules` without a `.gitignore`.
pub(super) fn bounded<const N: usize>(kinds: [Vec<String>; N], max: usize) -> (Vec<String>, bool) {
    let (mut kept, mut truncated) = (Vec::new(), false);
    for mut paths in kinds {
        // A file with conflicts is listed once for each side.
        paths.sort_unstable();
        paths.dedup();
        let room = max.saturating_sub(kept.len());
        truncated |= paths.len() > room;
        paths.truncate(room);
        kept.extend(paths);
    }
    kept.sort_unstable();
    (kept, truncated)
}

/// What's directly in the folder at `folder`, relative to the worktree at `worktree`: its files, and its folders, which
/// are read in turn. Refused unless it's a folder inside the worktree. A repository's own `.git` is left out, as git
/// leaves it out, and so is a name that isn't UTF-8, since it couldn't be asked for again.
pub(crate) fn read_folder(worktree: &Path, folder: &str) -> Result<WorktreeFiles, ApiError> {
    relative(folder)?;
    let found = crate::worktree_file::find_folder(worktree, folder)?;
    let failed = |e: io::Error| ApiError::new(ErrorCode::Worker, format!("couldn't read {folder}: {e}"));
    let prefix = folder.trim_end_matches('/');
    let (mut paths, mut folders) = (Vec::new(), Vec::new());
    for entry in std::fs::read_dir(&found).map_err(failed)? {
        let entry = entry.map_err(failed)?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        if name == ".git" {
            continue;
        }
        let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        // A link to a folder opens as one, and reading it is refused if it leads out of the worktree.
        if std::fs::metadata(entry.path()).is_ok_and(|metadata| metadata.is_dir()) {
            folders.push(path);
        } else {
            paths.push(path);
        }
    }
    folders.sort_unstable();
    let truncated = paths.len() + folders.len() > MAX_FILES;
    folders.truncate(MAX_FILES);
    let (paths, _) = bounded([paths], MAX_FILES - folders.len());
    Ok(WorktreeFiles { paths, folders, truncated })
}

/// What the file at `path`, relative to the worktree at `worktree`, holds. Refused unless it's a file inside the
/// worktree.
pub(crate) fn read(worktree: &Path, path: &str) -> Result<FileContents, ApiError> {
    relative(path)?;
    let file = crate::worktree_file::find(worktree, path)?;
    let failed = |e: io::Error| ApiError::new(ErrorCode::Worker, format!("couldn't read {path}: {e}"));
    let opened = std::fs::File::open(&file).map_err(failed)?;
    let size = |opened: &std::fs::File| opened.metadata().map(|metadata| metadata.len()).map_err(failed);
    let mut data = Vec::new();
    (&opened).take(length(SNIFF_BYTES)).read_to_end(&mut data).map_err(failed)?;
    let most = most_sent(&data);
    let bytes = size(&opened)?;
    if bytes > most {
        return Ok(FileContents::TooBig { bytes });
    }
    // A byte past the limit, so a file that has grown since it was measured is caught rather than cut short.
    (&opened).take((most + 1).saturating_sub(length(data.len()))).read_to_end(&mut data).map_err(failed)?;
    if length(data.len()) > most {
        return Ok(FileContents::TooBig { bytes: size(&opened)? });
    }
    Ok(contents(data))
}

/// What the file at `path` held in `commit`, from the repository of the worktree at `worktree`, as [`read`] gives a
/// file as it is now. A link isn't followed, so it holds the path it leads to.
pub(crate) async fn read_at(worktree: &Path, commit: &CommitId, path: &str) -> Result<FileContents, ApiError> {
    relative(path)?;
    let blob = crate::git::blob::find(worktree, commit, path).await?;
    // Only an image is sent past text's limit, and its start says whether it is one, so a big file that isn't is
    // refused without reading the rest.
    if blob.bytes > MAX_TEXT_BYTES && blob.bytes > most_sent(&blob.start(worktree, SNIFF_BYTES).await?) {
        return Ok(FileContents::TooBig { bytes: blob.bytes });
    }
    Ok(contents(blob.read(worktree).await?))
}

/// The most of a file that starts with `start` that a client is sent: an image's limit, or text's.
fn most_sent(start: &[u8]) -> u64 {
    if MediaType::sniff(&start[..start.len().min(SNIFF_BYTES)]).is_image() { MAX_IMAGE_BYTES } else { MAX_TEXT_BYTES }
}

/// What a file holds, from all of it: an image by what it starts with, and otherwise text, unless it's too big for
/// text or has a NUL byte near its start.
fn contents(data: Vec<u8>) -> FileContents {
    let start = &data[..data.len().min(SNIFF_BYTES)];
    let media_type = MediaType::sniff(start);
    if media_type.is_image() {
        return FileContents::Image { media_type, data: AttachmentBytes::new(data) };
    }
    if length(data.len()) > MAX_TEXT_BYTES {
        return FileContents::TooBig { bytes: length(data.len()) };
    }
    if start.contains(&0) {
        return FileContents::Binary;
    }
    FileContents::Text { text: String::from_utf8_lossy_owned(data) }
}

fn length(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

/// Refuses a path that isn't written as a list gives it, names between single slashes: one that's absolute, or that
/// has `.` or `..` in it, which would be read from somewhere else, or an empty name, which would name it differently
/// from how it's listed.
pub(super) fn relative(path: &str) -> Result<(), ApiError> {
    let names = path.split('/').all(|name| !matches!(name, "" | "." | ".."));
    let parts = Path::new(path).components();
    if !names || !parts.into_iter().all(|part| matches!(part, Component::Normal(_))) {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} isn't a path in the worktree")));
    }
    Ok(())
}
