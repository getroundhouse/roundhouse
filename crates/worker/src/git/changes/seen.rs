//! What the worker last read of a worktree's changed files, so it reads one again only once its size or modified time
//! differs from when it last looked.

use super::reading::{Read, read};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How long after a file's modified time the worker reads the file again each time it looks, whatever its size and
/// time say. A write in the same tick of a filesystem's clock as the look before it leaves the time as it was, and so
/// does one of the same size, so a time this recent can't tell the worker the file is as it was. It's longer than the
/// coarsest tick a worktree's filesystem is likely to have, FAT's two seconds.
const UNSETTLED: Duration = Duration::from_secs(2);

/// Each changed file of one worktree the worker has read, by its path as git writes it.
#[derive(Default)]
pub(super) struct Seen(HashMap<Vec<u8>, Looked>);

struct Looked {
    size: u64,
    modified: SystemTime,
    /// Whether `modified` was long enough before the look for a write since to have changed it, as [`UNSETTLED`] says.
    settled: bool,
    read: Read,
}

impl Seen {
    /// What the worktree at `worktree` holds at `path` as it is now, `at`, which is what `seen_now` keeps for the next
    /// look. It's what this held of it, unless the file has changed since by its size or modified time, or was too
    /// recent then to say. `None` where there's nothing to read: no file or link, or one the worker can't read, which
    /// leaves a client nothing to show of it either.
    pub(super) fn look(&self, seen_now: &mut Self, worktree: &Path, path: &[u8], at: SystemTime) -> Option<Read> {
        let full = worktree.join(path_from_git(path));
        // Unfollowed, since a link git tracks is the link and not what it names.
        let meta = std::fs::symlink_metadata(&full).ok()?;
        let (size, modified) = (meta.len(), meta.modified().ok()?);
        let kept = self.0.get(path).filter(|was| was.settled && was.size == size && was.modified == modified);
        let read = match kept {
            Some(was) => was.read.clone(),
            None => read(&full, &meta).ok()??,
        };
        let settled = at.duration_since(modified).is_ok_and(|age| age > UNSETTLED);
        seen_now.0.insert(path.to_vec(), Looked { size, modified, settled, read: read.clone() });
        Some(read)
    }
}

/// A path git writes, which on Unix is the bytes it has, whether or not they're UTF-8.
#[cfg(unix)]
fn path_from_git(path: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt as _;
    PathBuf::from(std::ffi::OsStr::from_bytes(path))
}

/// A path git writes, which Git for Windows writes as UTF-8, since a Windows path is always Unicode.
#[cfg(windows)]
fn path_from_git(path: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(path).into_owned())
}
