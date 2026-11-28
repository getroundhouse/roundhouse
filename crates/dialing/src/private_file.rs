//! Writing a file in the hub's data directory that only this user can read, whole or not at all.

use std::io::Write as _;
use std::path::Path;

/// Puts `bytes` at `path`, in place of what was there, by writing them beside it first and renaming that over it, so
/// whoever reads it reads the old file or the new one, never part of one. On Windows too, where the rename replaces
/// the file, and Rust opens a file to read it sharing its deletion, so a reader holding it open doesn't stop that.
pub(crate) fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let partial = path.with_extension("partial");
    let mut file = private_files::file_options().write(true).create(true).truncate(true).open(&partial)?;
    file.write_all(bytes).and_then(|()| file.sync_all())?;
    drop(file);
    std::fs::rename(&partial, path)
}
