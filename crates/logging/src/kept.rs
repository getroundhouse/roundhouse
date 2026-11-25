//! Keeping a log's file from growing without end.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// How long a log gets before it's moved aside: some months of a hub's, and short enough to read whole.
const LONGEST_BYTES: u64 = 10 * 1024 * 1024;

/// Moves the log at `path` aside if it's long, in place of the one moved aside before, for whoever is about to open
/// it. A process still writing to it goes on writing to the one that's moved. A log that can't be moved is left to
/// grow, since a log that's long is better than none.
pub fn move_aside_if_long(path: &Path) {
    if std::fs::metadata(path).is_ok_and(|log| log.len() > LONGEST_BYTES) {
        let _ = std::fs::rename(path, moved_aside(path));
    }
}

/// Where the log at `path` is moved to, such as `hub.log.1`.
pub(crate) fn moved_aside(path: &Path) -> PathBuf {
    let mut name = OsString::from(path);
    name.push(".1");
    name.into()
}
