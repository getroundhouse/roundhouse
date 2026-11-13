//! Unix's modes: reading and writing for the user alone on a file, and listing, searching and changing on a
//! directory.

use std::fs::{DirBuilder, OpenOptions, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::Path;

/// The mode of a file only this user can open.
const FILE_MODE: u32 = 0o600;

/// The mode of a directory only this user can list, search and change.
const DIR_MODE: u32 = 0o700;

/// Creates `path` and whatever is missing above it, each only this user's. One that's there already keeps its mode,
/// because `ROUNDHOUSE_HOME` can point anywhere, and changing the permissions of an existing directory could break
/// something unrelated.
pub fn create_dir_all(path: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(DIR_MODE).create(path)
}

/// Options that make a file they create only this user's, and leave one that's there already as it is. The caller
/// says how to open it.
pub fn file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.mode(FILE_MODE);
    options
}

/// Makes what's at `path` only this user's, for a file this process has just made another way than with
/// [`file_options`], such as a socket or a copy.
pub fn restrict(path: &Path) -> io::Result<()> {
    std::fs::set_permissions(path, Permissions::from_mode(FILE_MODE))
}
