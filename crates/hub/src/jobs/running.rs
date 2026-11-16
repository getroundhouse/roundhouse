use std::collections::HashMap;
use std::fmt::Display;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::Notify;
use tokio::task::{AbortHandle, JoinHandle};
use tracing::{error, info, warn};

use super::{JobKind, Progress};

/// The jobs running, by the key of what each is about, such as a workspace, so one about a key runs at a time.
pub(crate) struct Jobs<K> {
    /// `None` once the jobs have stopped, so none starts after.
    running: Arc<Mutex<Option<HashMap<K, Running>>>>,
}

struct Running {
    kind: JobKind,
    work: AbortHandle,
    /// What waits for the work within its limit and then runs its after-step.
    supervisor: AbortHandle,
    /// Tells the supervisor the job is called off, as [`Jobs::call_off`] says.
    called_off: Arc<Notify>,
}

/// How long a job's work has, and then its after-step.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub(crate) work: Duration,
    pub(crate) after: Duration,
}

/// How a job's work ended, for its after-step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ended {
    Finished,
    /// It was still running at its limit, and was stopped where it said it was. `began_uncut` says whether it had
    /// begun what it runs [`super::uncut`], which has ended by the time the after-step runs.
    Overran {
        at: String,
        began_uncut: bool,
    },
    /// It panicked, where it said it was.
    Panicked {
        at: String,
    },
    /// It was called off where it said it was, as [`Jobs::call_off`] says. `began_uncut` says whether it had begun what
    /// it runs [`super::uncut`], which has ended by the time the after-step runs.
    CalledOff {
        at: String,
        began_uncut: bool,
    },
}

impl Ended {
    /// Whether the work finished, or got as far as what it runs uncut, which then ran to its end, so what it would
    /// have done with what it waited for is done.
    pub(crate) fn finished_or_ran_uncut(&self) -> bool {
        matches!(
            self,
            Self::Finished | Self::Overran { began_uncut: true, .. } | Self::CalledOff { began_uncut: true, .. }
        )
    }
}

/// Why a job wasn't started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotStarted {
    /// One about the same key is running.
    KeyBusy,
    /// As many of its kind as may run at once are running.
    KindBusy,
    /// The jobs have stopped, as the hub does.
    Stopped,
}

impl<K: Clone + Eq + Hash + Display + Send + Sync + 'static> Jobs<K> {
    pub(crate) fn new() -> Self {
        Self { running: Arc::new(Mutex::new(Some(HashMap::new()))) }
    }

    /// Whether a job of `kind` about a key none is about would start now, so whatever decides what's due needn't look
    /// while it wouldn't.
    pub(crate) fn has_room(&self, kind: JobKind) -> bool {
        self.room_for(kind).is_ok()
    }

    /// The same, saying why one wouldn't, for what tells a person.
    pub(crate) fn room_for(&self, kind: JobKind) -> Result<(), NotStarted> {
        match self.running.lock().as_ref() {
            None => Err(NotStarted::Stopped),
            Some(running) if !Self::kind_has_room(running, kind) => Err(NotStarted::KindBusy),
            Some(_) => Ok(()),
        }
    }

    /// The keys of what jobs are running about, of any kind, for whatever decides what's due to pass over.
    pub(crate) fn busy(&self) -> Vec<K> {
        self.running.lock().as_ref().map_or_default(|running| running.keys().cloned().collect())
    }

    /// Starts `work`, which says where it has got to with [`super::step`], and has `limits.work` to finish. Then
    /// `after` runs, however the work ended, once the work is gone, within `limits.after`. Neither is waited on here,
    /// and the key is free again once both are done.
    pub(crate) fn start<W, A>(
        &self,
        kind: JobKind,
        key: K,
        limits: Limits,
        work: W,
        after: impl FnOnce(Ended) -> A + Send + 'static,
    ) -> Result<(), NotStarted>
    where
        W: Future<Output = ()> + Send + 'static,
        A: Future<Output = ()> + Send + 'static,
    {
        let mut guard = self.running.lock();
        let running = guard.as_mut().ok_or(NotStarted::Stopped)?;
        Self::room(running, kind, &key)?;
        let progress = Progress::default();
        let work = tokio::spawn(progress.scope(work));
        let work_task = work.abort_handle();
        // The supervisor takes the key off the list as it ends, which it can't before this lock is let go of, so the
        // key is always on the list first.
        let leaves = Leaves { running: self.running.clone(), key: key.clone() };
        let called_off = Arc::new(Notify::new());
        let watching = Watching { limits, progress, called_off: called_off.clone() };
        let supervisor = tokio::spawn(supervise(kind, leaves, watching, work, after));
        running.insert(key, Running { kind, work: work_task, supervisor: supervisor.abort_handle(), called_off });
        Ok(())
    }

    /// Ends the work of the job about `key` where it is, as its limit would, as when what it's for is no longer wanted,
    /// and then runs its after-step, told it was called off. What its work runs [`super::uncut`] runs to its end, as
    /// it does at the limit. Does nothing when no job is about `key`, or once its work has ended.
    pub(crate) fn call_off(&self, key: &K) {
        if let Some(job) = self.running.lock().as_ref().and_then(|running| running.get(key)) {
            job.called_off.notify_one();
        }
    }

    /// Ends every job where it is, without its after-step, and starts none after, as the hub stops.
    pub(crate) fn stop(&self) {
        let running = self.running.lock().take().unwrap_or_default();
        for job in running.into_values() {
            job.work.abort();
            job.supervisor.abort();
        }
    }

    fn room(running: &HashMap<K, Running>, kind: JobKind, key: &K) -> Result<(), NotStarted> {
        if running.contains_key(key) {
            return Err(NotStarted::KeyBusy);
        }
        if !Self::kind_has_room(running, kind) {
            return Err(NotStarted::KindBusy);
        }
        Ok(())
    }

    fn kind_has_room(running: &HashMap<K, Running>, kind: JobKind) -> bool {
        running.values().filter(|job| job.kind == kind).count() < kind.at_once()
    }
}

/// What the supervisor of a job watches its work by.
struct Watching {
    limits: Limits,
    progress: Progress,
    called_off: Arc<Notify>,
}

/// Waits for the work within its limit, stopping it there, or once it's called off, and then runs the after-step.
async fn supervise<K: Eq + Hash + Display, A: Future<Output = ()>>(
    kind: JobKind,
    leaves: Leaves<K>,
    watching: Watching,
    mut work: JoinHandle<()>,
    after: impl FnOnce(Ended) -> A,
) {
    let Watching { limits, progress, called_off } = watching;
    let key = &leaves.key;
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
        Some(Ok(Err(_))) => return,
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
}

/// Takes a job's key off the list of those running, however its supervisor ends.
struct Leaves<K: Eq + Hash> {
    running: Arc<Mutex<Option<HashMap<K, Running>>>>,
    key: K,
}

impl<K: Eq + Hash> Drop for Leaves<K> {
    fn drop(&mut self) {
        if let Some(running) = self.running.lock().as_mut() {
            running.remove(&self.key);
        }
    }
}
