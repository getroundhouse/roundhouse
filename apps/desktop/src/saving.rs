//! Saving what this Mac keeps for the app, such as the settings, on a thread of its own, in the order it changed, so
//! the newest is the one that stays. The app waits for it as it quits, and quitting holds the main thread, so nothing
//! here needs that thread.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// What saving failed on.
#[derive(Debug, thiserror::Error)]
#[error("{}: {source}", path.display())]
pub(crate) struct Failure {
    path: PathBuf,
    source: std::io::Error,
}

impl Failure {
    pub(crate) fn new(path: PathBuf, source: std::io::Error) -> Self {
        Self { path, source }
    }
}

/// Replaces the file at `path` with `value`, as JSON, all at once, so stopping partway leaves the old file rather than
/// half a new one.
pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Failure> {
    replace_with_json(path, value).map_err(|e| Failure::new(path.to_path_buf(), e))
}

fn replace_with_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, path)
}

enum Request<T> {
    Save(T),
    /// Answers once everything asked for before it is saved.
    Saved(flume::Sender<()>),
}

/// Saves what it's given, one at a time, on its own thread, which ends once this is dropped.
pub(crate) struct Saver<T> {
    requests: flume::Sender<Request<T>>,
}

impl<T: Send + 'static> Saver<T> {
    /// Starts the thread, which saves with `write`. A save that fails goes to the receiver returned.
    pub(crate) fn start(
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

    pub(crate) fn save(&self, value: T) {
        let _ = self.requests.send(Request::Save(value));
    }

    /// Hears once everything asked to be saved so far has been, or the thread has gone.
    pub(crate) fn saved(&self) -> flume::Receiver<()> {
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
