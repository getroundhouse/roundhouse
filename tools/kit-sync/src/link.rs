//! Symbolic links, which upstream's repository has, made and taken away as each platform does it.

use std::io;
use std::path::Path;

/// Makes a link at `at` to `target`, which is from the link's folder.
#[cfg(unix)]
pub(crate) fn link(target: &Path, at: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

/// Makes a link at `at` to `target`, which is from the link's folder. Windows makes a link to a folder differently
/// from one to a file, so it asks which `target` is.
#[cfg(windows)]
pub(crate) fn link(target: &Path, at: &Path) -> io::Result<()> {
    if at.parent().is_some_and(|folder| folder.join(target).is_dir()) {
        std::os::windows::fs::symlink_dir(target, at)
    } else {
        std::os::windows::fs::symlink_file(target, at)
    }
}

/// Takes away the file or link at `at`. Taking a link away leaves what it points to.
#[cfg(unix)]
pub(crate) fn unlink(at: &Path) -> io::Result<()> {
    std::fs::remove_file(at)
}

/// Takes away the file or link at `at`. Taking a link away leaves what it points to. Windows takes a link to a folder
/// away as it would a folder.
#[cfg(windows)]
pub(crate) fn unlink(at: &Path) -> io::Result<()> {
    let link_to_folder = std::fs::symlink_metadata(at).is_ok_and(|there| there.is_symlink())
        && std::fs::metadata(at).is_ok_and(|target| target.is_dir());
    if link_to_folder { std::fs::remove_dir(at) } else { std::fs::remove_file(at) }
}
