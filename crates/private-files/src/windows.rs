//! Windows' versions.

use std::fs::OpenOptions;
use std::io;
use std::path::Path;

/// Creates `path` and whatever is missing above it, which take the permissions of the folder each is made in.
pub fn create_dir_all(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Plain options, since a file they create takes its folder's permissions. The caller says how to open it.
pub fn file_options() -> OpenOptions {
    OpenOptions::new()
}

/// Nothing, since a file this process has just made has only the permissions it takes from its folder. A copy there
/// doesn't take its file's.
pub fn restrict(_path: &Path) -> io::Result<()> {
    Ok(())
}
