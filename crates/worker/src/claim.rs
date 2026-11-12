//! A worker's hold on its instance. One instance has one worker, which the hub keeps to, and this makes sure of it,
//! since a provider's `connect` can be asked again. It also records the worker, so its provider can tell whether one
//! is running there.

use std::fs::{File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

use process::ProcessRecord;
use protocol::paths::{WORKER_LOCK_FILE, WORKER_RECORD_FILE};

/// Held for as long as this worker serves the instance. The lock goes with the process.
#[derive(Debug)]
pub struct InstanceClaim {
    _lock: File,
}

#[derive(Debug, thiserror::Error)]
pub enum ClaimError {
    #[error("another worker is serving the instance in {0}")]
    Taken(PathBuf),
    /// What it was doing, and what went wrong.
    #[error("{what}")]
    Io { what: String, source: io::Error },
}

/// Claims the instance whose state is kept in `state_dir`, or refuses if another worker has it.
pub fn claim_instance(state_dir: &Path) -> Result<InstanceClaim, ClaimError> {
    let io = |what: String| move |source| ClaimError::Io { what, source };
    std::fs::create_dir_all(state_dir).map_err(io(format!("creating {}", state_dir.display())))?;
    let path = state_dir.join(WORKER_LOCK_FILE);
    let lock = File::create(&path).map_err(io(format!("opening {}", path.display())))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Err(ClaimError::Taken(state_dir.to_path_buf())),
        Err(TryLockError::Error(e)) => return Err(io(format!("locking {}", path.display()))(e)),
    }
    let record = state_dir.join(WORKER_RECORD_FILE);
    ProcessRecord::write(&record, std::process::id()).map_err(io(format!("writing {}", record.display())))?;
    Ok(InstanceClaim { _lock: lock })
}

/// Claims the instance whose state is kept in `state_dir`, for a worker that serves it on a socket. `None` if another
/// worker has it, which this leaves it to: one started at the same time, to serve the same hub, whose socket the
/// command that started this one waits for. It can't be one that's stopping, since the command starts no worker while
/// one holds the instance.
#[cfg(unix)]
pub fn claim_to_serve(state_dir: &Path) -> Result<Option<InstanceClaim>, ClaimError> {
    match claim_instance(state_dir) {
        Err(ClaimError::Taken(_)) => Ok(None),
        claimed => claimed.map(Some),
    }
}

/// Whether a worker holds the instance whose state is kept in `state_dir`, as one that's stopping does once it has
/// removed its socket.
#[cfg(unix)]
pub fn instance_claimed(state_dir: &Path) -> io::Result<bool> {
    let lock = match File::open(state_dir.join(WORKER_LOCK_FILE)) {
        Ok(lock) => lock,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    // Let go of at once, as the file closes.
    match lock.try_lock() {
        Ok(()) => Ok(false),
        Err(TryLockError::WouldBlock) => Ok(true),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    /// Claims the instance once its last claim has let go. A lock goes with the open file, and a process another
    /// thread is starting holds a copy of every open file until it has started, so a claim let go a moment ago can
    /// still read as taken. The worker's other tests start processes all the time.
    fn claim_once_let_go(state_dir: &Path) -> Result<InstanceClaim, ClaimError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match claim_instance(state_dir) {
                Err(ClaimError::Taken(_)) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
                claimed => return claimed,
            }
        }
    }

    #[test]
    fn an_instance_has_one_worker_at_a_time() {
        let dir = tempfile::tempdir().unwrap();

        let first = claim_instance(dir.path()).unwrap();
        let second = claim_instance(dir.path());
        drop(first);
        let after = claim_once_let_go(dir.path());

        assert!(matches!(second, Err(ClaimError::Taken(_))), "{second:?}");
        assert!(after.is_ok(), "{after:?}");
        let record = ProcessRecord::read(&dir.path().join(WORKER_RECORD_FILE)).unwrap();
        assert_eq!(record.pid, std::process::id());
        assert!(record.is_running());
    }

    #[cfg(unix)]
    #[test]
    fn an_instance_reads_as_claimed_only_while_a_worker_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!instance_claimed(dir.path()).unwrap(), "never claimed");
        let claim = claim_instance(dir.path()).unwrap();
        assert!(instance_claimed(dir.path()).unwrap());
        assert!(instance_claimed(dir.path()).unwrap(), "asking didn't let go of it");
        drop(claim);
        let deadline = Instant::now() + Duration::from_secs(5);
        while instance_claimed(dir.path()).unwrap() {
            assert!(Instant::now() < deadline, "still claimed once let go");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(claim_instance(dir.path()).is_ok(), "asking didn't keep it either");
    }

    /// As one started at the same time as another, to serve the same hub, finds the other has it.
    #[cfg(unix)]
    #[test]
    fn a_worker_leaves_an_instance_to_another_that_has_it() {
        let dir = tempfile::tempdir().unwrap();
        let _other = claim_instance(dir.path()).unwrap();

        let claimed = claim_to_serve(dir.path());

        assert!(matches!(claimed, Ok(None)), "{claimed:?}");
    }
}
