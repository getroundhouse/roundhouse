use std::fmt::Display;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use super::running::Table;
use super::{Ended, JobKind, Limits, Progress};

type Boxed = Pin<Box<dyn Future<Output = ()> + Send>>;

/// A job asked for that hasn't begun.
pub(super) struct Asked {
    kind: JobKind,
    limits: Limits,
    work: Boxed,
    after: Box<dyn FnOnce(Ended) -> Boxed + Send>,
}

/// A job whose work has begun, for its supervisor to watch.
pub(super) struct Run {
    kind: JobKind,
    limits: Limits,
    progress: Progress,
    pub(super) work: JoinHandle<()>,
    /// Tells the supervisor this run is called off, as [`super::Jobs::call_off`] says.
    pub(super) called_off: Arc<Notify>,
    after: Box<dyn FnOnce(Ended) -> Boxed + Send>,
}

impl Asked {
    pub(super) fn new<W, A>(
        kind: JobKind,
        limits: Limits,
        work: W,
        after: impl FnOnce(Ended) -> A + Send + 'static,
    ) -> Self
    where
        W: Future<Output = ()> + Send + 'static,
        A: Future<Output = ()> + Send + 'static,
    {
        Self { kind, limits, work: Box::pin(work), after: Box::new(move |ended| Box::pin(after(ended))) }
    }

    /// Begins the work, on a task of its own.
    pub(super) fn begin(self) -> Run {
        let progress = Progress::default();
        let work = tokio::spawn(progress.scope(self.work));
        let called_off = Arc::new(Notify::new());
        Run { kind: self.kind, limits: self.limits, progress, work, called_off, after: self.after }
    }
}

/// Watches each run of the job about a key in turn: the one begun, and then each asked for again while the one before
/// it ran, and takes the key off the table once no more was asked for.
pub(super) async fn supervise<K: Eq + Hash + Display>(mut leaves: Leaves<K>, first: Run) {
    let mut run = first;
    loop {
        if watch(&leaves.key, run).await == Watched::Stopped {
            return;
        }
        match leaves.next() {
            Some(next) => run = next,
            None => return,
        }
    }
}

/// Whether the jobs stopped while a run was watched.
#[derive(PartialEq, Eq)]
enum Watched {
    Ended,
    Stopped,
}

/// Waits for the work within its limit, stopping it there, or once it's called off, and then runs the after-step.
async fn watch<K: Display>(key: &K, run: Run) -> Watched {
    let Run { kind, limits, progress, mut work, called_off, after } = run;
    // `None` once it's called off.
    let ran = tokio::select! {
        ran = tokio::time::timeout(limits.work, &mut work) => Some(ran),
        () = called_off.notified() => None,
    };
    let (ended, stopping) = match ran {
        Some(Ok(Ok(()))) => (Ended::Finished, None),
        Some(Ok(Err(e))) if e.is_panic() => {
            let at = progress.last();
            error!(%key, %at, "a job of the hub's panicked: {kind}");
            (Ended::Panicked { at }, None)
        }
        // Stopped, as the hub stops, which ends this task too.
        Some(Ok(Err(_))) => return Watched::Stopped,
        Some(Err(_)) => {
            let at = progress.last();
            warn!(%key, %at, limit = ?limits.work, "a job of the hub's didn't finish in time, so it was stopped: {kind}");
            work.abort();
            (Ended::Overran { at, began_uncut: false }, Some(work))
        }
        None => {
            let at = progress.last();
            info!(%key, %at, "a job of the hub's was called off: {kind}");
            work.abort();
            (Ended::CalledOff { at, began_uncut: false }, Some(work))
        }
    };
    let afterwards = async {
        // Once a stopped job's work is gone, so what it held is let go of before the after-step puts things back.
        if let Some(work) = stopping {
            super::step("waiting for its work to stop");
            let _ = work.await;
        }
        // And what it runs uncut, which goes on, so the after-step doesn't undo what that does.
        super::step("waiting for what its work runs uncut");
        let began = progress.uncut_ended().await;
        let mut ended = ended;
        if let Ended::Overran { began_uncut, .. } | Ended::CalledOff { began_uncut, .. } = &mut ended {
            *began_uncut = began;
        }
        super::step("putting back what its work left, before it said how");
        after(ended).await;
    };
    // Where the after-step says it is, too, for the log if it doesn't finish.
    let afterwards = progress.scope(afterwards);
    if tokio::time::timeout(limits.after, afterwards).await.is_err() {
        let at = progress.last();
        warn!(%key, %at, limit = ?limits.after, "a job of the hub's didn't finish putting things back in time: {kind}");
    }
    Watched::Ended
}

/// Takes a job's key off the table, as its last run ends, however its supervisor ends.
pub(super) struct Leaves<K: Eq + Hash> {
    running: Table<K>,
    key: K,
    /// Whether [`Leaves::next`] took the key off already, after which another job about it may have begun.
    left: bool,
}

impl<K: Eq + Hash> Leaves<K> {
    pub(super) fn new(running: Table<K>, key: K) -> Self {
        Self { running, key, left: false }
    }

    /// Begins the job asked for again about the key while the last run went, if one was, in that run's place on the
    /// table, and otherwise takes the key off. `None` once the key is off, or the jobs have stopped.
    fn next(&mut self) -> Option<Run> {
        let mut guard = self.running.lock();
        let entry = guard.as_mut()?.get_mut(&self.key)?;
        let Some(asked) = entry.again.take() else {
            guard.as_mut().and_then(|running| running.remove(&self.key));
            self.left = true;
            return None;
        };
        let run = asked.begin();
        (entry.work, entry.called_off) = (run.work.abort_handle(), run.called_off.clone());
        Some(run)
    }
}

impl<K: Eq + Hash> Drop for Leaves<K> {
    fn drop(&mut self) {
        if self.left {
            return;
        }
        if let Some(running) = self.running.lock().as_mut() {
            running.remove(&self.key);
        }
    }
}
