use super::super::temporary_prefix;
use protocol::StampedBuild;
use protocol::paths::{Paths, hub_copy_files, is_hub_copy_dir};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How long a copy is kept once made, even with nothing marking it as run from. A process of a build from before
/// processes marked the file they run from marks nothing, and a worker outlives as many hubs as its workspace stays at
/// work through, over an hour of updates being usual. A day of them is a few copies.
const KEEP_FOR: Duration = Duration::from_hours(24);

/// How long a temporary copy can go unchanged before it's taken for one a client was killed while making. Making one
/// takes well under a second.
pub(super) const ABANDONED_AFTER: Duration = Duration::from_mins(10);

/// Removes each copy of the daemon in `paths` that no process runs from and that's older than [`KEEP_FOR`], but
/// `own`'s, with the directory it was in, and every temporary copy that was abandoned. What can't be removed now is
/// tried again the next time.
pub(in super::super) fn remove_unused(paths: &Paths, own: &StampedBuild) {
    let own = paths.hub_copy(own);
    let now = SystemTime::now();
    for dir in build_dirs(&paths.daemons_dir) {
        remove_abandoned(&dir, now);
        if own.parent() != Some(dir.as_path()) {
            remove_if_unused(&dir, now);
        }
    }
}

/// Each build's directory in `daemons_dir`. Anything else there is left alone.
fn build_dirs(daemons_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(daemons_dir) else { return Vec::new() };
    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let named = entry.file_name().to_str().is_some_and(is_hub_copy_dir);
            (named && entry.file_type().ok()?.is_dir()).then(|| entry.path())
        })
        .collect()
}

/// Removes the copy in `dir`, by whichever name it has, and then `dir` if that leaves it empty, unless the copy was
/// made within [`KEEP_FOR`] of `now` or a process runs from it.
fn remove_if_unused(dir: &Path, now: SystemTime) {
    if hub_copy_files().iter().all(|file| gone_unless_used(&dir.join(file), now)) {
        // Only if it's empty, so not while a client is making a copy in it.
        let _ = std::fs::remove_dir(dir);
    }
}

/// Whether nothing is at `copy` once it's removed, unless it was made within [`KEEP_FOR`] of `now` or a process runs
/// from it. It's removed while it's claimed, so nothing starting from it can mark it first.
fn gone_unless_used(copy: &Path, now: SystemTime) -> bool {
    match process::claim_unused(copy) {
        Ok(Some(_claimed)) => !changed_within(copy, KEEP_FOR, now) && std::fs::remove_file(copy).is_ok(),
        Ok(None) => false,
        // None by this name, and maybe only ever a temporary copy.
        Err(e) => e.kind() == io::ErrorKind::NotFound,
    }
}

/// Removes each temporary copy in `dir` that, by `now`, has gone unchanged for [`ABANDONED_AFTER`].
fn remove_abandoned(dir: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let prefix = temporary_prefix();
    for path in entries.filter_map(|entry| Some(entry.ok()?.path())) {
        let temporary = path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with(&prefix));
        if temporary && !changed_within(&path, ABANDONED_AFTER, now) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Whether the file at `path` changed within `span` of `now`, or may have, as when when it changed can't be read, or is
/// after `now`.
fn changed_within(path: &Path, span: Duration, now: SystemTime) -> bool {
    let Ok(changed) = std::fs::metadata(path).and_then(|file| file.modified()) else { return true };
    !now.duration_since(changed).is_ok_and(|since| since >= span)
}
