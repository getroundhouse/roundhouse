//! The one thread that writes the store's changes to the cache file, in the order they were made, so a change waits on
//! the file outside the lock every list takes.

use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use tokio::sync::oneshot;
use tracing::warn;

use super::file::{self, Write};
use super::kinds::Kind;
use crate::cache_file::CacheFile;

/// One write to make to the file.
type Queued = Box<dyn FnOnce(&CacheFile) + Send>;

/// Writes the file what each change queues, one after another, in the order they were queued. Dropping it waits for
/// every write queued before, as a test does that opens another hub on the same file.
pub(super) struct Writer {
    /// The queue, and the thread that writes it. `None` where the thread couldn't be started, so what the file would
    /// keep is in memory alone, as where the file can't be made, and once the writer has been dropped.
    writing: Option<(mpsc::Sender<Queued>, JoinHandle<()>)>,
}

impl Writer {
    /// A writer of `file`, on a thread of its own, since each write can wait on the file for as long as its busy
    /// timeout, while another process holds it.
    pub(super) fn new(file: Arc<CacheFile>) -> Self {
        let (queue, queued) = mpsc::channel::<Queued>();
        let thread = std::thread::Builder::new().name("observations-file".into()).spawn(move || {
            for write in queued {
                write(&file);
            }
        });
        let writing = thread
            .inspect_err(|error| warn!("keeping observations in memory alone, with no thread to write them: {error}"))
            .ok()
            .map(|thread| (queue, thread));
        Self { writing }
    }

    /// Queues `write` of the observation of `of`, which is made once every write queued before it is. One the file
    /// refuses is only logged, as [`file::write`] says. Nothing of a kind kept in memory alone, which has no table.
    pub(super) fn write<K: Kind>(&self, of: K::Of, write: Write<K::Value>) {
        if K::IN_MEMORY_ALONE {
            return;
        }
        self.queue(Box::new(move |file| file::write::<K>(file, &of, write)));
    }

    /// Waits until every write queued before has been made, as the hub does as it stops.
    pub(super) async fn written(&self) {
        let (done, made) = oneshot::channel();
        self.queue(Box::new(move |_| {
            // The waiter has given up, as when the hub stopping was dropped, which leaves nothing to tell.
            let _ = done.send(());
        }));
        // Ends with an error only where nothing is written, so there's nothing to wait for.
        let _ = made.await;
    }

    fn queue(&self, write: Queued) {
        let Some((queue, _)) = &self.writing else { return };
        if queue.send(write).is_err() {
            warn!("keeping an observation in memory alone, since the thread that writes the cache file has ended");
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let Some((queue, thread)) = self.writing.take() else { return };
        // Ends the thread once it has made every write queued.
        drop(queue);
        if thread.join().is_err() {
            warn!("the thread that writes the cache file's observations panicked");
        }
    }
}
