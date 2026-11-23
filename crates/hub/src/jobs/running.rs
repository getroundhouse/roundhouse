use std::collections::HashMap;
use std::fmt::Display;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::Notify;
use tokio::task::AbortHandle;

use super::JobKind;
use super::kind::AskedAgain;
use super::supervising::{Asked, Leaves, supervise};

/// The jobs running, by the key of what each is about, such as a workspace, so one about a key runs at a time.
pub(crate) struct Jobs<K> {
    running: Table<K>,
}

/// The jobs running, by key, which their supervisors take theirs off as they end. `None` once the jobs have stopped,
/// so none starts after.
pub(super) type Table<K> = Arc<Mutex<Option<HashMap<K, Running>>>>;

pub(super) struct Running {
    pub(super) kind: JobKind,
    /// The work of the run going now, which is the job asked for again once it has begun.
    pub(super) work: AbortHandle,
    /// What waits for the work within its limit and then runs its after-step, and then begins the job asked for
    /// again, if one was.
    supervisor: AbortHandle,
    /// Tells the supervisor the run going now is called off, as [`Jobs::call_off`] says.
    pub(super) called_off: Arc<Notify>,
    /// The last job of its kind asked for about the key since this one began, of a kind that runs once more, as
    /// [`AskedAgain::RunsOnceMore`] says, which begins once this one has ended.
    pub(super) again: Option<Asked>,
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
    /// One about the same key is running, and the job's kind is one that's refused then, as
    /// [`AskedAgain::Refused`] says, or the one running is of another kind.
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

    /// The keys of what jobs are running about, of any kind, for whatever decides what's due to pass over. A key stays
    /// on it until the job asked for again about it, if one was, has ended too.
    pub(crate) fn busy(&self) -> Vec<K> {
        self.running.lock().as_ref().map_or_default(|running| running.keys().cloned().collect())
    }

    /// Starts `work`, which says where it has got to with [`super::step`], and has `limits.work` to finish. Then
    /// `after` runs, however the work ended, once the work is gone, within `limits.after`. Neither is waited on here,
    /// and the key is free again once both are done. Asked for while one of its kind about `key` runs, it's refused,
    /// or it starts once that one has ended, as the kind's [`AskedAgain`] says, in place of any asked for meanwhile.
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
        let asked = Asked::new(kind, limits, work, after);
        let mut guard = self.running.lock();
        let running = guard.as_mut().ok_or(NotStarted::Stopped)?;
        if let Some(busy) = running.get_mut(&key) {
            if busy.kind != kind || kind.asked_again() == AskedAgain::Refused {
                return Err(NotStarted::KeyBusy);
            }
            // What it takes the place of goes once the lock is let go of, since dropping it drops what its work holds.
            let replaced = busy.again.replace(asked);
            drop(guard);
            drop(replaced);
            return Ok(());
        }
        if !Self::kind_has_room(running, kind) {
            return Err(NotStarted::KindBusy);
        }
        let run = asked.begin();
        let (work, called_off) = (run.work.abort_handle(), run.called_off.clone());
        // The supervisor takes the key off the table as it ends, which it can't before this lock is let go of, so the
        // key is always on the table first.
        let supervisor = tokio::spawn(supervise(Leaves::new(self.running.clone(), key.clone()), run));
        running.insert(key, Running { kind, work, supervisor: supervisor.abort_handle(), called_off, again: None });
        Ok(())
    }

    /// Ends the work of the job about `key` where it is, as its limit would, as when what it's for is no longer wanted,
    /// and then runs its after-step, told it was called off. What its work runs [`super::uncut`] runs to its end, as
    /// it does at the limit. One asked for again about `key` is let go of, unbegun, since it's for the same. Does
    /// nothing when no job is about `key`, or once its work has ended.
    pub(crate) fn call_off(&self, key: &K) {
        let mut guard = self.running.lock();
        let Some(job) = guard.as_mut().and_then(|running| running.get_mut(key)) else { return };
        job.called_off.notify_one();
        let again = job.again.take();
        drop(guard);
        drop(again);
    }

    /// Ends every job where it is, without its after-step, and starts none after, nor any asked for again, as the hub
    /// stops. The next hub puts back what a job left, as it starts: it ends a worker nothing carries on with, and
    /// records a sign-in left waiting as ended.
    pub(crate) fn stop(&self) {
        let running = self.running.lock().take().unwrap_or_default();
        for job in running.into_values() {
            job.work.abort();
            job.supervisor.abort();
        }
    }

    fn kind_has_room(running: &HashMap<K, Running>, kind: JobKind) -> bool {
        running.values().filter(|job| job.kind == kind).count() < kind.at_once()
    }
}
