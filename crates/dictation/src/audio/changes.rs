//! Changes a recording makes to the system while it's dictated to, such as muting the sound output, each made and
//! undone on a thread of its own for its kind, since each waits on another process, which can be slow or wedged, such
//! as `coreaudiod` while it restarts, and the app's thread mustn't wait on it. One thread makes and undoes every change
//! of its kind, in the order they're asked for, so a change never overtakes the undoing of the one before it.

use parking_lot::Mutex;

/// One kind of change, and the thread that makes it, from the first change on. The thread lasts as long as the app
/// does, since this keeps the sender of its jobs.
pub(crate) struct Changes<D> {
    /// Names the thread.
    thread: &'static str,
    /// Makes the change, and gives what undoes it to `undo`. `None` where there's nothing to undo.
    make: fn() -> Option<D>,
    undo: fn(D),
    jobs: Mutex<Option<flume::Sender<Job>>>,
}

/// A change [`Changes::hold`] asked for, undone once this is dropped, if it was made.
pub(crate) struct Held {
    /// Never sent on. Dropping it tells the thread to undo the change.
    _release: flume::Sender<()>,
}

/// What a thread is asked to do.
enum Job {
    /// Make the change, and undo it once `released` disconnects, before any later job.
    Hold { released: flume::Receiver<()> },
    /// Answer once every job before it is done.
    Restored(flume::Sender<()>),
}

impl<D: 'static> Changes<D> {
    pub(crate) const fn new(thread: &'static str, make: fn() -> Option<D>, undo: fn(D)) -> Self {
        Self { thread, make, undo, jobs: Mutex::new(None) }
    }

    /// Has the change made, without waiting for it. `None` where the thread couldn't start, so it won't be.
    pub(crate) fn hold(&self) -> Option<Held> {
        let (release, released) = flume::bounded(0);
        let mut jobs = self.jobs.lock();
        if jobs.is_none() {
            let (sender, taken) = flume::unbounded();
            let (make, undo) = (self.make, self.undo);
            let thread = std::thread::Builder::new().name(self.thread.to_string());
            thread.spawn(move || work(&taken, make, undo)).ok()?;
            *jobs = Some(sender);
        }
        let sent = jobs.as_ref().is_some_and(|jobs| jobs.send(Job::Hold { released }).is_ok());
        sent.then_some(Held { _release: release })
    }

    /// Answers once every change made before it, and let go of, is undone, for the app to wait for as it quits, since
    /// the thread ends with the app. Disconnects instead where the thread has gone, which ends the wait as well.
    pub(crate) fn restored(&self) -> flume::Receiver<()> {
        let (answer, answered) = flume::bounded(1);
        let jobs = self.jobs.lock().clone();
        match jobs {
            Some(jobs) => {
                let _ = jobs.send(Job::Restored(answer));
            }
            // Nothing has been changed.
            None => {
                let _ = answer.send(());
            }
        }
        answered
    }
}

/// Does each job in turn: makes the change with `make`, and once that's let go of, undoes with `undo` what it changed.
fn work<D>(jobs: &flume::Receiver<Job>, make: impl Fn() -> Option<D>, undo: impl Fn(D)) {
    for job in jobs {
        match job {
            Job::Hold { released } => {
                let changed = make();
                // Nothing is sent, so this returns once the `Held` is dropped.
                let _ = released.recv();
                if let Some(changed) = changed {
                    undo(changed);
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

    use super::{Changes, Job, work};

    /// Long enough for anything a test waits on, which takes microseconds.
    const TIMEOUT: Duration = Duration::from_secs(10);

    /// A thread doing jobs whose change gives `changed`, what it's done in order, and word of each change as it's made.
    fn thread(changed: Option<u32>) -> (flume::Sender<Job>, Arc<Mutex<Vec<String>>>, flume::Receiver<()>) {
        let (jobs, taken) = flume::unbounded();
        let done = Arc::new(Mutex::new(Vec::new()));
        let (tells, making) = flume::unbounded();
        let (by_make, by_undo) = (done.clone(), done.clone());
        let make = move || {
            by_make.lock().push("make".to_string());
            let _ = tells.send(());
            changed
        };
        let undo = move |change: u32| by_undo.lock().push(format!("undo {change}"));
        std::thread::spawn(move || work(&taken, make, undo));
        (jobs, done, making)
    }

    #[test]
    fn what_is_changed_is_undone_once_let_go_of_and_before_anything_asked_after() {
        let (jobs, done, making) = thread(Some(7));
        let (release, released) = flume::bounded(0);
        jobs.send(Job::Hold { released }).unwrap();
        let (answer, restored) = flume::bounded(1);
        jobs.send(Job::Restored(answer)).unwrap();
        making.recv_timeout(TIMEOUT).unwrap();
        assert!(restored.is_empty(), "nothing is restored while the change is held");

        drop(release);

        restored.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(*done.lock(), ["make", "undo 7"]);
    }

    #[test]
    fn what_was_not_changed_is_not_undone() {
        let (jobs, done, _) = thread(None);
        let (release, released) = flume::bounded(0);
        jobs.send(Job::Hold { released }).unwrap();
        drop(release);
        let (answer, restored) = flume::bounded(1);

        jobs.send(Job::Restored(answer)).unwrap();

        restored.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(*done.lock(), ["make"]);
    }

    /// What [`CHANGES`] has undone.
    static UNDONE: Mutex<Vec<u32>> = Mutex::new(Vec::new());

    static CHANGES: Changes<u32> = Changes::new("test-changes", || Some(3), |change| UNDONE.lock().push(change));

    #[test]
    fn a_held_change_is_undone_on_its_thread_once_dropped() {
        let held = CHANGES.hold().unwrap();

        drop(held);

        CHANGES.restored().recv_timeout(TIMEOUT).unwrap();
        assert_eq!(*UNDONE.lock(), [3]);
    }
}
