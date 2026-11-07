//! Stopping a hub without talking to it. A hub of another version can't be asked anything, and needs
//! stopping for one that matches to start, so this goes through the record it keeps and a signal, and
//! no request has to stay compatible across versions.

use std::fs::{File, TryLockError};
use std::time::{Duration, Instant};

use process::{ProcessRecord, Signal, signal_process};
use protocol::paths::Paths;

/// A hub gives each worker time to stop its agents before exiting, and a worker that doesn't answer
/// takes a few seconds to give up on.
const STOP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum StopHubError {
    /// No record, or the process it names has gone or isn't the one recorded.
    #[error("couldn't find the hub's process")]
    NotFound,
    #[error("hub {pid} was told to stop, and is still running")]
    StillRunning { pid: u32 },
}

/// The hub recorded for these paths, if it's still running.
pub fn running_hub_pid(paths: &Paths) -> Option<u32> {
    ProcessRecord::read(&paths.pid).filter(ProcessRecord::is_running).map(|record| record.pid)
}

/// Asks the hub to stop the way a terminal would, which still stops its workers and agents, and blocks
/// until it has. Returns its PID.
pub fn stop_hub_by_signal(paths: &Paths) -> Result<u32, StopHubError> {
    stop_within(paths, STOP_TIMEOUT)
}

fn stop_within(paths: &Paths, timeout: Duration) -> Result<u32, StopHubError> {
    let pid = running_hub_pid(paths).ok_or(StopHubError::NotFound)?;
    signal_process(pid, Signal::Terminate);
    let deadline = Instant::now() + timeout;
    while !lock_is_free(paths) {
        if Instant::now() >= deadline {
            return Err(StopHubError::StillRunning { pid });
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(pid)
}

/// A hub holds this until it exits, and a new one can't start before then. Its record can't say when
/// that is: a process stays listed until its parent collects it, and the client that started the hub
/// never does.
fn lock_is_free(paths: &Paths) -> bool {
    let Ok(lock) = File::open(&paths.lock) else { return true };
    !matches!(lock.try_lock(), Err(TryLockError::WouldBlock))
}

#[cfg(test)]
mod tests {
    use std::process::{Child, Command};

    use super::*;

    fn paths_in(dir: &std::path::Path) -> Paths {
        let home = dir.to_str().unwrap().to_string();
        protocol::paths::resolve(|name| match name {
            protocol::paths::ENV_HOME | protocol::paths::ENV_WORKSPACES => Some(home.clone()),
            _ => None,
        })
        .unwrap()
    }

    fn sleeper() -> Child {
        Command::new("sleep").arg("30").spawn().unwrap()
    }

    #[test]
    fn a_pid_reused_by_another_process_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        let mut other = sleeper();
        std::fs::write(&paths.pid, format!("{}\nlong ago\n", other.id())).unwrap();

        assert_eq!(running_hub_pid(&paths), None);
        assert_eq!(stop_hub_by_signal(&paths), Err(StopHubError::NotFound));

        assert!(matches!(other.try_wait(), Ok(None)), "the signal reached a process that isn't the hub");
        other.kill().unwrap();
        other.wait().unwrap();
    }

    #[test]
    fn no_record_means_nothing_to_stop() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(stop_hub_by_signal(&paths_in(dir.path())), Err(StopHubError::NotFound));
    }

    #[test]
    fn a_hub_that_keeps_its_lock_is_reported_rather_than_waited_on_forever() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        let mut hub = sleeper();
        process::ProcessRecord::write(&paths.pid, hub.id()).unwrap();
        let lock = File::create(&paths.lock).unwrap();
        lock.try_lock().unwrap();

        let stopped = stop_within(&paths, Duration::from_millis(50));

        assert_eq!(stopped, Err(StopHubError::StillRunning { pid: hub.id() }));
        let _ = hub.kill();
        hub.wait().unwrap();
    }

    #[test]
    fn a_hub_has_gone_once_its_lock_is_free() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        assert!(lock_is_free(&paths), "with no lock file, nothing holds it");

        let lock = File::create(&paths.lock).unwrap();
        lock.try_lock().unwrap();
        assert!(!lock_is_free(&paths));

        drop(lock);
        assert!(lock_is_free(&paths));
    }
}
