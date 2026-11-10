//! Files and directories that only this user can open, as the data directory and what's in it are. Windows has no
//! modes: what's created there gets its directory's permissions, which under a user's profile keep other users out.

use std::fs::{DirBuilder, OpenOptions};
use std::io;
use std::path::Path;

/// Creates `path` and whatever is missing above it. It only sets the mode on directories it creates, because
/// `ROUNDHOUSE_HOME` can point anywhere, and changing the permissions of an existing directory could break something
/// unrelated.
pub(crate) fn create_dir_all(path: &Path) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

/// Options that give a file they create a mode only this user can open it with. The caller says how to open it.
pub(crate) fn file_options() -> OpenOptions {
    #[cfg(unix)]
    {
        let mut options = OpenOptions::new();
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        options
    }
    #[cfg(windows)]
    OpenOptions::new()
}
