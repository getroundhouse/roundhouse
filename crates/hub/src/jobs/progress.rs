use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use tokio::sync::{OwnedRwLockReadGuard, RwLock};

tokio::task_local! {
    /// Where the job whose work is running on this task has got to.
    static CURRENT: Progress;
}

/// Says what the job this runs in is doing now, such as which call it's waiting on and from which worker, so the log
/// can say where one that didn't finish had stopped. Does nothing outside a job, so code that jobs share with requests
/// can say it all the same.
pub(crate) fn step(step: impl Into<String>) {
    let _ = CURRENT.try_with(|progress| *progress.step.lock() = step.into());
}

/// Where a job has got to, as [`step`] last said, and what of its work runs [`super::uncut`].
#[derive(Clone, Default)]
pub(super) struct Progress {
    step: Arc<Mutex<String>>,
    /// Held to read by each part of the work that runs uncut, for as long as it runs.
    uncut: Arc<RwLock<()>>,
    began_uncut: Arc<AtomicBool>,
}

impl Progress {
    /// Runs `work` as the job this is the progress of, so what it says with [`step`] is kept here.
    pub(super) fn scope<F: Future>(&self, work: F) -> impl Future<Output = F::Output> + use<F> {
        CURRENT.scope(self.clone(), work)
    }

    /// What it last said, or that it said nothing.
    pub(super) fn last(&self) -> String {
        let step = self.step.lock();
        if step.is_empty() { "before it said what it was doing".to_string() } else { step.clone() }
    }

    /// For a part of the work of the job running on this task that's to run uncut, held for as long as it runs, so
    /// the job waits for it. `None` outside a job.
    pub(super) fn uncut_begins() -> Option<OwnedRwLockReadGuard<()>> {
        let held = CURRENT.try_with(|progress| {
            progress.began_uncut.store(true, Ordering::SeqCst);
            // Only a job's work begins one, and the job waits for them only once its work has gone, so this is free.
            progress.uncut.clone().try_read_owned().ok()
        });
        held.ok().flatten()
    }

    /// Once every part of the work that runs uncut has ended, whether any began.
    pub(super) async fn uncut_ended(&self) -> bool {
        drop(self.uncut.write().await);
        self.began_uncut.load(Ordering::SeqCst)
    }
}
