//! Making a name given or taken in a directory last through a power cut, which the hub's logins and backups need, since
//! what's kept under the name would go with it.

use std::fs::File;
use std::io;
use std::path::Path;

/// Makes a name given or taken in `dir` last through a power cut. Not on Windows, which has no way to ask for that
/// through std: a directory can't be opened as a file there, so this would fail.
pub(crate) fn sync_dir(dir: &Path) -> io::Result<()> {
    if cfg!(windows) {
        return Ok(());
    }
    File::open(dir).and_then(|handle| handle.sync_all())
}

/// Creates `dir` and whatever is missing above it, only this user's, as [`private_files::create_dir_all`] does, and
/// has `dir`'s name last through a power cut, or what's kept in it would go with it. Nothing for one that's there.
pub(crate) fn create_dir_all(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    private_files::create_dir_all(dir)?;
    dir.parent().map_or(Ok(()), sync_dir)
}
