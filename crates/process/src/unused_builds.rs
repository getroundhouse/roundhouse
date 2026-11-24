//! Removing the builds that nothing runs from anymore, from a directory that holds each build's programs in a
//! directory of its own, as a client's copies of the daemon are kept, and the builds a hub installed as a service runs.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How long a build is kept once made, even with nothing marking it as run from: one that's placed and not yet run
/// from, as a client's copy is until the hub it starts marks it, and one run from by a process that couldn't mark it as
/// it started, which goes on unmarked. Placing a build and starting from it take seconds.
const KEEP_FOR: Duration = Duration::from_hours(24);

/// How long a temporary file can go unchanged before it's taken for one whoever was making it was killed while doing
/// so. Making one takes well under a second.
pub const ABANDONED_AFTER: Duration = Duration::from_mins(10);

/// A directory of builds, each in a directory of its own.
pub struct Builds<'a> {
    /// Where each build's directory is.
    pub dir: &'a Path,
    /// Whether a name in `dir` is a build's directory. Anything else there is left alone.
    pub is_build: fn(&str) -> bool,
    /// Each name a program can have in a build's directory. A build goes only once none of them is run from, and the
    /// rest stay once one is, so the one processes run from comes first.
    pub programs: &'a [String],
    /// What a temporary file in a build's directory starts with, as a program has on its way in.
    pub temporary_prefix: &'a str,
}

impl Builds<'_> {
    /// Removes each build that no process runs from and that's older than [`KEEP_FOR`], but those whose directories
    /// are in `keep`, with the directory it was in, and every temporary file that was abandoned. What can't be removed
    /// now is tried again the next time.
    pub fn remove_unused(&self, keep: &[PathBuf]) {
        let now = SystemTime::now();
        for dir in self.build_dirs() {
            self.remove_abandoned(&dir, now);
            if !keep.contains(&dir) {
                self.remove_if_unused(&dir, now);
            }
        }
    }

    /// Each build's directory.
    fn build_dirs(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(self.dir) else { return Vec::new() };
        entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let named = entry.file_name().to_str().is_some_and(self.is_build);
                (named && entry.file_type().ok()?.is_dir()).then(|| entry.path())
            })
            .collect()
    }

    /// Removes the programs in `dir`, and then `dir` if that leaves it empty, unless one was made within [`KEEP_FOR`]
    /// of `now` or a process runs from it.
    fn remove_if_unused(&self, dir: &Path, now: SystemTime) {
        if self.programs.iter().all(|program| gone_unless_used(&dir.join(program), now)) {
            // Only if it's empty, so not while a program is on its way in.
            let _ = std::fs::remove_dir(dir);
        }
    }

    /// Removes each temporary file in `dir` that, by `now`, has gone unchanged for [`ABANDONED_AFTER`].
    fn remove_abandoned(&self, dir: &Path, now: SystemTime) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for path in entries.filter_map(|entry| Some(entry.ok()?.path())) {
            let name = path.file_name().and_then(|name| name.to_str());
            let temporary = name.is_some_and(|name| name.starts_with(self.temporary_prefix));
            if temporary && !changed_within(&path, ABANDONED_AFTER, now) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// Whether nothing is at `program` once it's removed, unless it was made within [`KEEP_FOR`] of `now` or a process
/// runs from it. It's removed while it's claimed, so nothing starting from it can mark it first.
fn gone_unless_used(program: &Path, now: SystemTime) -> bool {
    match crate::claim_unused(program) {
        Ok(Some(_claimed)) => !changed_within(program, KEEP_FOR, now) && std::fs::remove_file(program).is_ok(),
        Ok(None) => false,
        // None by this name, and maybe only ever a temporary file.
        Err(e) => e.kind() == io::ErrorKind::NotFound,
    }
}

/// Whether the file at `path` changed within `span` of `now`, or may have, as when when it changed can't be read, or is
/// after `now`.
fn changed_within(path: &Path, span: Duration, now: SystemTime) -> bool {
    let Ok(changed) = std::fs::metadata(path).and_then(|file| file.modified()) else { return true };
    !now.duration_since(changed).is_ok_and(|since| since >= span)
}
