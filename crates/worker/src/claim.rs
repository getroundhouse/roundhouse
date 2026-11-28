//! A worker's hold on its instance. One instance has one worker, which the hub keeps to, and this makes sure of it,
//! since a provider's `connect` can be asked again. It also records the worker, so its provider can tell whether one
//! is running there.

use std::fs::{File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use process::ProcessRecord;
use protocol::paths::{WORKER_LOCK_FILE, WORKER_RECORD_FILE};

/// How long a worker that finds the claim held looks again before it takes the claim to be another worker's. The local
/// provider's `worker_state` takes the same lock, shared, for as long as it takes to see whether anyone holds it, which
/// is moments. So a worker that starts as the hub asks never reads that as another worker having the instance, and
/// leaves it to one that does. A worker that really is a second one waits this long for nothing.
const LOOK_AGAIN_FOR: Duration = Duration::from_millis(500);

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

/// Claims the instance for a worker whose state directory is `state_dir`, or refuses if another worker has it.
pub fn claim_instance(state_dir: &Path) -> Result<InstanceClaim, ClaimError> {
    let io = |what: String| move |source| ClaimError::Io { what, source };
    std::fs::create_dir_all(state_dir).map_err(io(format!("creating {}", state_dir.display())))?;
    let path = state_dir.join(WORKER_LOCK_FILE);
    let lock = File::create(&path).map_err(io(format!("opening {}", path.display())))?;
    match lock_unless_held(&lock) {
        Ok(true) => {}
        Ok(false) => return Err(ClaimError::Taken(state_dir.to_path_buf())),
        Err(e) => return Err(io(format!("locking {}", path.display()))(e)),
    }
    let record = state_dir.join(WORKER_RECORD_FILE);
    ProcessRecord::write(&record, std::process::id()).map_err(io(format!("writing {}", record.display())))?;
    Ok(InstanceClaim { _lock: lock })
}

/// Takes the lock, or says it's held, once it has looked for [`LOOK_AGAIN_FOR`] and found it so every time.
#[expect(
    clippy::disallowed_methods,
    reason = "the pace a claim that's held is looked at again, in case what holds it is only the provider's look"
)]
fn lock_unless_held(lock: &File) -> io::Result<bool> {
    let deadline = Instant::now() + LOOK_AGAIN_FOR;
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(true),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(TryLockError::WouldBlock) => return Ok(false),
            Err(TryLockError::Error(e)) => return Err(e),
        }
    }
}

/// Claims the instance for a worker whose state directory is `state_dir`, and that outlives its hub, as one that dials
/// does. `None` if another worker has it, which this leaves it to: one already serving the hub, or started at the same
/// time to serve it.
pub fn claim_to_serve(state_dir: &Path) -> Result<Option<InstanceClaim>, ClaimError> {
    match claim_instance(state_dir) {
        Err(ClaimError::Taken(_)) => Ok(None),
        claimed => claimed.map(Some),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claims the instance once its last claim has let go. A lock goes with the open file, and a process another
    /// thread is starting holds a copy of every open file until it has started, so a claim let go a moment ago can
    /// still read as taken. The worker's other tests start processes all the time.
    fn claim_once_let_go(state_dir: &Path) -> Result<InstanceClaim, ClaimError> {
        let mut claimed = None;
        waiting::thread::up_to(Duration::from_secs(5), || {
            let tried = claim_instance(state_dir);
            let taken = matches!(tried, Err(ClaimError::Taken(_)));
            claimed = Some(tried);
            !taken
        });
        claimed.expect("a wait looks at least once")
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

    /// As the local provider's look at whether a worker holds the instance, which holds the lock shared for a moment,
    /// does as a worker starts.
    #[test]
    fn a_look_at_the_claim_doesnt_stop_a_worker_taking_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(WORKER_LOCK_FILE), "").unwrap();
        let looked = File::open(dir.path().join(WORKER_LOCK_FILE)).unwrap();
        looked.try_lock_shared().unwrap();
        let looking = std::thread::spawn(move || {
            waiting::thread::for_what_mustnt_happen(Duration::from_millis(100));
            drop(looked);
        });

        let claimed = claim_instance(dir.path());

        looking.join().unwrap();
        assert!(claimed.is_ok(), "{claimed:?}");
    }

    /// As one started at the same time as another, to serve the same hub, finds the other has it.
    #[test]
    fn a_worker_leaves_an_instance_to_another_that_has_it() {
        let dir = tempfile::tempdir().unwrap();
        let _other = claim_instance(dir.path()).unwrap();

        let claimed = claim_to_serve(dir.path());

        assert!(matches!(claimed, Ok(None)), "{claimed:?}");
    }
}
