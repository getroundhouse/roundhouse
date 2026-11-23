//! Reading what the worktree holds of a changed file: a version of it, and, as git would count them, its lines.

use protocol::model::FileVersion;
use sha1_smol::Sha1;
use std::fs::{File, Metadata};
use std::io::{self, Read as _};
use std::path::Path;
use std::time::UNIX_EPOCH;

/// What git's `core.bigFileThreshold` is unless a person sets it: git takes a file larger than this as binary, and
/// counts none of its lines. The worker reads none of such a file either, so a build's output it doesn't ignore isn't
/// read whole each time it grows.
pub(super) const MAX_READ_BYTES: u64 = 512 * 1024 * 1024;

/// How far into a file git looks for a NUL byte, which makes it binary.
const BINARY_SNIFF_BYTES: usize = 8000;

#[derive(Clone, Debug)]
pub(super) struct Read {
    pub(super) version: FileVersion,
    /// The lines it holds, as git counts a new file's, or `None` where git would count none.
    pub(super) lines: Option<u64>,
}

/// What the worktree holds at `path`, which `meta` says, unfollowed, is there: a file's content, or the path a link
/// names, which is what git keeps of a link. `None` for anything else, such as a submodule, which is a folder.
pub(super) fn read(path: &Path, meta: &Metadata) -> io::Result<Option<Read>> {
    if meta.is_symlink() {
        let target = std::fs::read_link(path)?;
        let target = target.as_os_str().as_encoded_bytes();
        let mut content = Content::new(len(target));
        content.add(target);
        return Ok(Some(content.finish()));
    }
    if !meta.is_file() {
        return Ok(None);
    }
    if meta.len() > MAX_READ_BYTES {
        return Ok(Some(Read { version: by_size_and_time(meta)?, lines: None }));
    }
    let mut file = File::open(path)?;
    let mut content = Content::new(meta.len());
    let mut buffer = vec![0; 64 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => return Ok(Some(content.finish())),
            Ok(n) => content.add(&buffer[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

/// The version of what the worktree at `worktree` holds at `path` now, as a summary names it, or `None` where there's
/// nothing to read there, or it's outside the worktree. The folders on the way are followed, and the last link isn't,
/// since git tracks a link as itself.
pub(in crate::git) fn version_now(worktree: &Path, path: &Path) -> Option<FileVersion> {
    let named = worktree.join(path);
    let folder = named.parent()?.canonicalize().ok()?;
    if !folder.starts_with(worktree.canonicalize().ok()?) {
        return None;
    }
    let found = folder.join(named.file_name()?);
    let meta = std::fs::symlink_metadata(&found).ok()?;
    Some(read(&found, &meta).ok()??.version)
}

/// For a file too large to read, which changes when either does, so it changes when the file is only touched too. It
/// can't be taken for a hash, which has no `-`.
fn by_size_and_time(meta: &Metadata) -> io::Result<FileVersion> {
    let modified = meta.modified()?.duration_since(UNIX_EPOCH).map_or(0, |since| since.as_nanos());
    Ok(FileVersion::from(format!("{}-{modified}", meta.len())))
}

/// A file's bytes as they're read, hashed and counted as they come.
struct Content {
    hash: Sha1,
    read: usize,
    newlines: u64,
    binary: bool,
    last: Option<u8>,
}

impl Content {
    /// Hashed as git hashes a blob of `len` bytes, so in a repository of SHA-1 objects a version is the ID
    /// `git hash-object --no-filters` gives the file.
    fn new(len: u64) -> Self {
        let mut hash = Sha1::new();
        hash.update(format!("blob {len}\0").as_bytes());
        Self { hash, read: 0, newlines: 0, binary: false, last: None }
    }

    fn add(&mut self, bytes: &[u8]) {
        self.hash.update(bytes);
        let sniffed = BINARY_SNIFF_BYTES.saturating_sub(self.read).min(bytes.len());
        self.binary |= memchr::memchr(0, &bytes[..sniffed]).is_some();
        self.newlines += count(memchr::memchr_iter(b'\n', bytes).count());
        self.last = bytes.last().copied().or(self.last);
        self.read = self.read.saturating_add(bytes.len());
    }

    /// git counts a last line with no newline after it as a line.
    fn finish(self) -> Read {
        let unended = u64::from(self.last.is_some_and(|last| last != b'\n'));
        let lines = (!self.binary).then_some(self.newlines + unended);
        Read { version: FileVersion::from(self.hash.digest().to_string()), lines }
    }
}

fn len(bytes: &[u8]) -> u64 {
    count(bytes.len())
}

/// A usize is at most 64 bits wherever the worker builds.
fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}
