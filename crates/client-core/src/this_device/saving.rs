//! Saving what a device keeps for an app, such as its settings, on a thread of its own, in the order it changed, so the
//! newest is the one that stays, and reading it back. An app waits for what's saved as it quits, which can hold its main
//! thread, so nothing here needs that thread.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// What reading or saving a file failed on.
#[derive(Debug, thiserror::Error)]
#[error("{}: {source}", path.display())]
pub struct NotKept {
    path: PathBuf,
    source: std::io::Error,
}

impl NotKept {
    pub fn new(path: &Path, source: std::io::Error) -> Self {
        Self { path: path.to_path_buf(), source }
    }
}

/// What's saved at `path`, or `None` when nothing is.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, NotKept> {
    let failed = |source| NotKept::new(path, source);
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(failed(e)),
    };
    serde_json::from_str(&text).map(Some).map_err(|e| failed(e.into()))
}

/// Replaces the file at `path` with `value`, as JSON, all at once, so stopping partway leaves the old file rather than
/// half a new one.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), NotKept> {
    replace_with_json(path, value).map_err(|e| NotKept::new(path, e))
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
pub struct Saver<T> {
    requests: flume::Sender<Request<T>>,
}

impl<T: Send + 'static> Saver<T> {
    /// Starts the thread, which saves with `write`, and hands what a save failed on to `failed`, on that thread.
    pub fn start(
        mut write: impl FnMut(&T) -> Result<(), NotKept> + Send + 'static,
        mut failed: impl FnMut(NotKept) + Send + 'static,
    ) -> Self {
        let (requests, received) = flume::unbounded();
        std::thread::spawn(move || {
            for request in received {
                match request {
                    Request::Save(value) => {
                        if let Err(failure) = write(&value) {
                            failed(failure);
                        }
                    }
                    Request::Saved(done) => {
                        let _ = done.send(());
                    }
                }
            }
        });
        Self { requests }
    }

    pub fn save(&self, value: T) {
        let _ = self.requests.send(Request::Save(value));
    }

    /// Hears once everything asked to be saved so far has been, or the thread has gone.
    pub fn saved(&self) -> flume::Receiver<()> {
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
        let (done, seen) = flume::unbounded();
        let write = move |value: &u32| {
            let _ = wait.recv();
            let _ = done.send(*value);
            Ok(())
        };
        let saver = Saver::start(write, |failure| panic!("{failure}"));

        saver.save(1);
        saver.save(2);
        let all_saved = saver.saved();

        assert!(all_saved.try_recv().is_err(), "the first save is still writing");
        go.send(()).unwrap();
        go.send(()).unwrap();
        all_saved.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(seen.try_iter().collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn a_save_that_fails_says_so_and_the_next_still_happens() {
        let (done, seen) = flume::unbounded();
        let (said, failed) = flume::unbounded();
        let write = move |value: &u32| {
            let _ = done.send(*value);
            let full = || NotKept::new(Path::new("settings.json"), std::io::Error::other("full"));
            if *value == 1 { Err(full()) } else { Ok(()) }
        };
        let saver = Saver::start(write, move |failure| drop(said.send(failure)));

        saver.save(1);
        saver.save(2);
        saver.saved().recv_timeout(TIMEOUT).unwrap();

        assert!(failed.try_recv().is_ok());
        assert_eq!(seen.try_iter().collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn what_was_never_saved_isnt_what_cant_be_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kept.json");

        assert_eq!(read_json::<u32>(&path).unwrap(), None);
        write_json(&path, &7).unwrap();
        assert_eq!(read_json(&path).unwrap(), Some(7));
        std::fs::write(&path, "{ not json").unwrap();
        assert!(read_json::<u32>(&path).is_err());
    }
}
