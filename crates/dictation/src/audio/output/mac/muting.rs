//! Muting on a thread of its own, since each of Core Audio's calls waits on `coreaudiod`, which can be slow or wedged,
//! such as while it restarts, and the app's thread mustn't wait on it. One thread makes every call, in the order
//! they're asked for, so a mute never overtakes the unmute before it.

use parking_lot::Mutex;

use super::device;

/// Whether this platform can mute its sound output while it's dictated to. A Mac can.
pub const MUTES_OUTPUT: bool = true;

/// The sound output [`mute_output`] asked to have muted, unmuted once this is dropped, if it was muted.
pub(crate) struct MutedOutput {
    /// Never sent on. Dropping it tells the thread to unmute.
    _release: flume::Sender<()>,
}

/// What the thread is asked to do.
enum Job {
    /// Mute the device sound plays from, and unmute it once `released` disconnects, before any later job.
    Mute { released: flume::Receiver<()> },
    /// Answer once every job before it is done.
    Restored(flume::Sender<()>),
}

/// The thread's jobs, from the first mute on. The thread lasts as long as the app does, since this keeps their sender.
static JOBS: Mutex<Option<flume::Sender<Job>>> = Mutex::new(None);

/// Has the device sound plays from muted, without waiting for it. `None` where the thread couldn't start, so nothing
/// will be. Otherwise the thread mutes it unless it's muted already or can't be muted, as [`device::mute_default`]
/// says.
pub(crate) fn mute_output() -> Option<MutedOutput> {
    let (release, released) = flume::bounded(0);
    let mut jobs = JOBS.lock();
    if jobs.is_none() {
        let (sender, taken) = flume::unbounded();
        let thread = std::thread::Builder::new().name("dictation-sound-output".to_string());
        thread.spawn(move || work(&taken, device::mute_default, device::unmute)).ok()?;
        *jobs = Some(sender);
    }
    let sent = jobs.as_ref().is_some_and(|jobs| jobs.send(Job::Mute { released }).is_ok());
    sent.then_some(MutedOutput { _release: release })
}

/// Answers once everything muted before it, and let go of, is unmuted, for the app to wait for as it quits, since the
/// thread ends with the app. Disconnects instead where the thread has gone, which ends the wait as well.
pub fn output_restored() -> flume::Receiver<()> {
    let (answer, answered) = flume::bounded(1);
    let jobs = JOBS.lock().clone();
    match jobs {
        Some(jobs) => {
            let _ = jobs.send(Job::Restored(answer));
        }
        // Nothing has been muted.
        None => {
            let _ = answer.send(());
        }
    }
    answered
}

/// Does each job in turn: mutes with `mute`, and once that's let go of, unmutes with `unmute` what it muted.
fn work<D>(jobs: &flume::Receiver<Job>, mute: impl Fn() -> Option<D>, unmute: impl Fn(D)) {
    for job in jobs {
        match job {
            Job::Mute { released } => {
                let muted = mute();
                // Nothing is sent, so this returns once the `MutedOutput` is dropped.
                let _ = released.recv();
                if let Some(device) = muted {
                    unmute(device);
                }
            }
            Job::Restored(answer) => {
                let _ = answer.send(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use parking_lot::Mutex;

    use super::{Job, work};

    /// Long enough for anything a test waits on, which takes microseconds.
    const TIMEOUT: Duration = Duration::from_secs(10);

    /// A thread doing jobs whose mute gives `muted`, what it's done in order, and word of each mute as it's done.
    fn thread(muted: Option<u32>) -> (flume::Sender<Job>, Arc<Mutex<Vec<String>>>, flume::Receiver<()>) {
        let (jobs, taken) = flume::unbounded();
        let done = Arc::new(Mutex::new(Vec::new()));
        let (tells, muting) = flume::unbounded();
        let (by_mute, by_unmute) = (done.clone(), done.clone());
        let mute = move || {
            by_mute.lock().push("mute".to_string());
            let _ = tells.send(());
            muted
        };
        let unmute = move |device: u32| by_unmute.lock().push(format!("unmute {device}"));
        std::thread::spawn(move || work(&taken, mute, unmute));
        (jobs, done, muting)
    }

    #[test]
    fn what_is_muted_is_unmuted_once_let_go_of_and_before_anything_asked_after() {
        let (jobs, done, muting) = thread(Some(7));
        let (release, released) = flume::bounded(0);
        jobs.send(Job::Mute { released }).unwrap();
        let (answer, restored) = flume::bounded(1);
        jobs.send(Job::Restored(answer)).unwrap();
        muting.recv_timeout(TIMEOUT).unwrap();
        assert!(restored.is_empty(), "nothing is restored while the mute is held");

        drop(release);

        restored.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(*done.lock(), ["mute", "unmute 7"]);
    }

    #[test]
    fn what_was_not_muted_is_not_unmuted() {
        let (jobs, done, _) = thread(None);
        let (release, released) = flume::bounded(0);
        jobs.send(Job::Mute { released }).unwrap();
        drop(release);
        let (answer, restored) = flume::bounded(1);

        jobs.send(Job::Restored(answer)).unwrap();

        restored.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(*done.lock(), ["mute"]);
    }
}
