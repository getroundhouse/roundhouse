//! The job a shell runs in front, which an interactive shell starts in a process group of its own, and which can
//! outlast the shell.

#[cfg(unix)]
use std::time::Duration;

use portable_pty::MasterPty;
#[cfg(unix)]
use tokio::time::Instant;

/// How often to ask whether a job that has to end by a deadline has. Often enough that a stop takes barely longer than
/// the job does to end, and asking is one system call.
#[cfg(unix)]
const STILL_RUNNING_EVERY: Duration = Duration::from_millis(20);

/// The group of the job the shell runs in front, if it isn't the shell's own.
#[derive(Clone, Copy)]
pub(super) enum Front {
    /// As it was last looked at, while the shell runs.
    Looked(Option<u32>),
    /// As it was when the shell was hung up on, which is the job a stop ends. Nothing looks again after, since what the
    /// shell runs in front as it exits, such as its exit trap's commands, isn't that job.
    HungUpOn(Option<u32>),
}

impl Front {
    pub(super) fn group(self) -> Option<u32> {
        match self {
            Front::Looked(group) | Front::HungUpOn(group) => group,
        }
    }
}

/// The group in front, which a PTY says only while its shell runs.
#[cfg(unix)]
pub(super) fn in_front(master: &dyn MasterPty) -> Option<u32> {
    master.process_group_leader().and_then(|group| u32::try_from(group).ok())
}

/// None, since Windows has no groups.
#[cfg(windows)]
pub(super) fn in_front(_master: &dyn MasterPty) -> Option<u32> {
    None
}

/// Waits for the job in `front`, which can outlast the shell, and kills it if it hasn't ended by `kill_at`.
#[cfg(unix)]
#[expect(
    clippy::disallowed_methods,
    reason = "the pace a job that outlasted its shell is looked at again until it ends or its grace is up"
)]
pub(super) async fn outlasted_by(front: Front, kill_at: Instant) {
    let Some(group) = front.group() else { return };
    while process::group_running(group) {
        if Instant::now() >= kill_at {
            process::signal_group(group, process::Signal::Kill);
            return;
        }
        tokio::time::sleep(STILL_RUNNING_EVERY).await;
    }
}
