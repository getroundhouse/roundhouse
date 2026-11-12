//! Telling a program's file that a process runs from, which mustn't be removed, from one nothing runs from anymore. A
//! process marks the file it runs from with a shared lock on it, for as long as it runs, and whoever wants to remove
//! the file first claims it with an exclusive lock, which it only gets while nothing has it marked.

use std::fs::{File, TryLockError};
use std::io;
use std::path::Path;

/// The file a process runs from, marked as in use until this is dropped, which the process exiting does too.
#[must_use = "the file is only marked for as long as this is kept"]
pub struct RunningFrom {
    _marked: File,
}

impl RunningFrom {
    /// Marks `program`, the file this process runs from. It fails while someone has the file claimed to remove it.
    pub fn mark(program: &Path) -> io::Result<Self> {
        let file = File::open(program)?;
        file.try_lock_shared().map_err(io::Error::from)?;
        Ok(Self { _marked: file })
    }
}

/// A file claimed to be removed, since nothing has it marked, and that nothing can mark until this is dropped.
#[must_use = "the file is only claimed for as long as this is kept"]
pub struct Claimed {
    _claimed: File,
}

/// Claims the file at `path` to remove it, unless a process has it marked as the one it runs from.
pub fn claim_unused(path: &Path) -> io::Result<Option<Claimed>> {
    let file = File::open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(Claimed { _claimed: file })),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn a_marked_file_cannot_be_claimed_until_the_mark_goes() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let marked = RunningFrom::mark(file.path()).unwrap();

        let while_marked = claim_unused(file.path()).unwrap();
        drop(marked);
        let after = claimed_once_free(file.path());

        assert!(while_marked.is_none());
        assert!(after.is_some());
    }

    /// Claims `path` once it can be, or gives up. A mark only goes once every process holding it has let go, and a
    /// child that another test is starting holds what this process has open until it runs its program.
    fn claimed_once_free(path: &Path) -> Option<Claimed> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match claim_unused(path).unwrap() {
                None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                claimed => return claimed,
            }
        }
    }

    #[test]
    fn a_claimed_file_cannot_be_marked() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let _claimed = claim_unused(file.path()).unwrap().unwrap();

        assert!(RunningFrom::mark(file.path()).is_err());
    }

    #[test]
    fn a_file_can_be_marked_more_than_once() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let _first = RunningFrom::mark(file.path()).unwrap();

        assert!(RunningFrom::mark(file.path()).is_ok());
    }
}
