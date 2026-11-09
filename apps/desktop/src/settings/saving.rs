//! Saving the settings on a thread of their own, in the order they changed, so the newest is the one that stays. The
//! app waits for it as it quits, and quitting holds the main thread, so nothing here needs that thread.

use std::path::PathBuf;

/// What saving failed on.
#[derive(Debug, thiserror::Error)]
#[error("{}: {source}", path.display())]
pub(super) struct Failure {
    path: PathBuf,
    source: std::io::Error,
}

impl Failure {
    pub(super) fn new(path: PathBuf, source: std::io::Error) -> Self {
        Self { path, source }
    }
}

enum Request<T> {
    Save(T),
    /// Answers once everything asked for before it is saved.
    Saved(flume::Sender<()>),
}

/// Saves what it's given, one at a time, on its own thread, which ends once this is dropped.
pub(super) struct Saver<T> {
    requests: flume::Sender<Request<T>>,
}

impl<T: Send + 'static> Saver<T> {
    /// Starts the thread, which saves with `write`. A save that fails goes to the receiver returned.
    pub(super) fn start(
        mut write: impl FnMut(&T) -> Result<(), Failure> + Send + 'static,
    ) -> (Self, flume::Receiver<Failure>) {
        let (requests, received) = flume::unbounded();
        let (failures, failed) = flume::unbounded();
        std::thread::spawn(move || {
            for request in received {
                match request {
                    Request::Save(value) => {
                        if let Err(failure) = write(&value) {
                            let _ = failures.send(failure);
                        }
                    }
                    Request::Saved(done) => {
                        let _ = done.send(());
                    }
                }
            }
        });
        (Self { requests }, failed)
    }

    pub(super) fn save(&self, value: T) {
        let _ = self.requests.send(Request::Save(value));
    }

    /// Hears once everything asked to be saved so far has been, or the thread has gone.
    pub(super) fn saved(&self) -> flume::Receiver<()> {
        let (done, saved) = flume::bounded(1);
        let _ = self.requests.send(Request::Saved(done));
        saved
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn saved_answers_once_everything_before_it_is_saved_in_order() {
        let (go, wait) = flume::unbounded::<()>();
        let (wrote, written) = flume::unbounded();
        let (saver, _) = Saver::start(move |value: &u32| {
            let _ = wait.recv();
            let _ = wrote.send(*value);
            Ok(())
        });

        saver.save(1);
        saver.save(2);
        let all_saved = saver.saved();

        assert!(all_saved.try_recv().is_err(), "the first save is still writing");
        go.send(()).unwrap();
        go.send(()).unwrap();
        all_saved.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(written.try_iter().collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn a_save_that_fails_says_so_and_the_next_still_happens() {
        let (wrote, written) = flume::unbounded();
        let (saver, failed) = Saver::start(move |value: &u32| {
            let _ = wrote.send(*value);
            if *value == 1 { Err(Failure::new("settings.json".into(), std::io::Error::other("full"))) } else { Ok(()) }
        });

        saver.save(1);
        saver.save(2);
        saver.saved().recv_timeout(TIMEOUT).unwrap();

        assert!(failed.try_recv().is_ok());
        assert_eq!(written.try_iter().collect::<Vec<_>>(), [1, 2]);
    }
}
