use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// A folder that one holder at a time writes to: an exclusive lock on the `.lock` file in it, for as long as this
/// lives. A second holder, in this process or another on the same folder, waits for it. The lock goes with the
/// process, so a crash leaves none behind.
///
/// The `.lock` file and its folder are never removed: a new `.lock` would be a new file to lock, and let a second holder
/// in while the first still writes.
pub struct FolderLock {
    dir: PathBuf,
    _file: File,
}

impl FolderLock {
    /// Waits for `dir`'s lock, and makes `dir` first if it isn't there. Taking it again while holding it waits forever.
    pub fn take(dir: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let file = OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(".lock"))?;
        file.lock()?;
        Ok(Self { dir: dir.to_path_buf(), _file: file })
    }

    /// The folder it locks.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}
