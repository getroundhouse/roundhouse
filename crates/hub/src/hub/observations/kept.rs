//! What the store keeps of each observation in memory, and what a change to one makes of it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::{ApiError, ObservationNotice};
use protocol::model::{Level, Observation, Observed, Rev};
use tokio::sync::Mutex as AsyncMutex;
use tracing::warn;

use super::file::{self, Row};
use super::kinds::{Form, Kind, WorkspaceChanges, WorkspaceLines};
use crate::cache_file::CacheFile;
use crate::store::Store;

/// Every observation the hub holds, in memory, and in the cache file.
pub(crate) struct Observations {
    /// Held while an observation changes, from before it takes its revision until its notice is sent, and while every
    /// record is listed, so a list has every observation whose revision it takes in. An observation and a record are
    /// in different files, so a change can't take its revision in a record's transaction.
    pub(super) changing: AsyncMutex<()>,
    pub(super) tables: Mutex<Tables>,
    pub(super) file: Arc<CacheFile>,
}

/// Each kind's table, in memory.
#[derive(Default)]
pub(crate) struct Tables {
    pub(super) workspace_lines: Table<WorkspaceLines>,
    pub(super) workspace_changes: Table<WorkspaceChanges>,
}

/// What's kept of one kind of observation.
pub(crate) struct Table<K: Kind> {
    kept: HashMap<K::Of, Kept<K::Value>>,
    /// What something keeps up to date now, such as a worker watching a worktree, so what's read of it is current.
    kept_current: HashSet<K::Of>,
}

impl<K: Kind> Default for Table<K> {
    fn default() -> Self {
        Self { kept: HashMap::new(), kept_current: HashSet::new() }
    }
}

/// One observation, as its revision says it.
struct Kept<V> {
    said: Said<V>,
    rev: Rev,
}

/// What an observation says, and when it was read.
#[derive(Clone)]
pub(super) struct Said<V> {
    pub(super) value: V,
    /// Whether it's read as something keeps it up to date.
    current: bool,
    /// When it was read, as clients are told while it isn't current: the read that found what it says, or, once
    /// nothing keeps it current, when nothing did any more. A later read that finds the same moves no revision, so it moves this
    /// neither, and every client of one revision is told the same.
    as_of_ms: i64,
    /// When it was last read, which the cache file keeps.
    pub(super) read_at_ms: i64,
}

impl<V: Clone> Said<V> {
    fn observed(&self) -> Observed<V> {
        let value = self.value.clone();
        if self.current { Observed::Current { value } } else { Observed::AsOf { value, as_of_ms: self.as_of_ms } }
    }
}

/// A change the code that reads something makes to an observation.
pub(super) enum Change<V> {
    /// It was read, and found `value`.
    Read { value: V, read_at_ms: i64 },
    /// Something keeps it up to date from now on, so what's read of it from now is current.
    KeptCurrent,
    /// Nothing keeps it up to date any more, since `as_of_ms`, so it's as of then: what kept it up to date would have
    /// said of any change before. As of its last read where that's later, as one that landed since can be.
    LetGo { as_of_ms: i64 },
    /// What it's of is gone.
    Gone,
}

/// What a change makes of an observation, worked out before it takes a revision.
pub(super) enum Step<V> {
    /// Nothing to keep or tell.
    Same,
    /// It says what it said, read again since, which the file keeps. Nothing is told, and its revision stays.
    Reread(Said<V>),
    /// It says something new, or is gone, under a new revision.
    Changed(Changed<V>),
}

/// What changes under a new revision.
pub(super) enum Changed<V> {
    Says(Said<V>),
    Gone,
}

impl<K: Kind> Table<K> {
    /// What `change` makes of the observation of `of`. Whether something keeps it current changes here, since that's
    /// told of only through what's read.
    pub(super) fn step(&mut self, of: &K::Of, change: Change<K::Value>) -> Step<K::Value> {
        let kept = self.kept.get(of).map(|kept| &kept.said);
        match change {
            Change::Read { value, read_at_ms } => {
                let current = self.kept_current.contains(of);
                match kept {
                    // A read that began before the one kept, and lands after it, as a slow one can.
                    Some(kept) if kept.read_at_ms > read_at_ms => Step::Same,
                    Some(kept) if kept.value == value && kept.current == current => {
                        Step::Reread(Said { read_at_ms, ..kept.clone() })
                    }
                    _ => Step::Changed(Changed::Says(Said { value, current, as_of_ms: read_at_ms, read_at_ms })),
                }
            }
            Change::KeptCurrent => {
                // What was read before isn't current, so it's not said to be until the next read.
                self.kept_current.insert(of.clone());
                Step::Same
            }
            Change::LetGo { as_of_ms } => {
                self.kept_current.remove(of);
                match kept {
                    Some(kept) if kept.current => {
                        let at = as_of_ms.max(kept.read_at_ms);
                        Step::Changed(Changed::Says(Said {
                            current: false,
                            as_of_ms: at,
                            read_at_ms: at,
                            ..kept.clone()
                        }))
                    }
                    Some(_) | None => Step::Same,
                }
            }
            Change::Gone => {
                self.kept_current.remove(of);
                if kept.is_some() { Step::Changed(Changed::Gone) } else { Step::Same }
            }
        }
    }

    /// Whether something keeps the observation of `of` up to date now.
    pub(super) fn kept_current(&self, of: &K::Of) -> bool {
        self.kept_current.contains(of)
    }

    /// Keeps when the observation of `of` was read again, as `said` says, under the revision it has.
    pub(super) fn reread(&mut self, of: &K::Of, said: Said<K::Value>) {
        if let Some(kept) = self.kept.get_mut(of) {
            kept.said = said;
        }
    }

    /// Keeps what `changed` made of the observation of `of`, at `rev`, and returns what clients are to be told of it.
    pub(super) fn change(&mut self, of: &K::Of, changed: Changed<K::Value>, rev: Rev) -> ObservationNotice {
        match changed {
            Changed::Says(said) => {
                let observation = K::observation(of, said.observed(), rev);
                self.kept.insert(of.clone(), Kept { said, rev });
                ObservationNotice::Copy(observation)
            }
            Changed::Gone => {
                self.kept.remove(of);
                ObservationNotice::Removed { observation: K::key(of), rev }
            }
        }
    }
}

/// What the store does with every kind's table alike.
trait AnyTable {
    fn form(&self) -> Form;
    /// Each observation it keeps that's sent at `level`.
    fn listed(&self, level: &Level) -> Vec<Observation>;
    /// Keeps what the cache file kept, each as of when it was read, at `rev`.
    fn load(&mut self, file: &CacheFile, rev: Rev) -> rusqlite::Result<()>;
}

impl<K: Kind> AnyTable for Table<K> {
    fn form(&self) -> Form {
        K::FORM
    }

    fn listed(&self, level: &Level) -> Vec<Observation> {
        let at_level = self.kept.iter().filter(|(of, _)| K::key(of).level().as_ref() == Some(level));
        at_level.map(|(of, kept)| K::observation(of, kept.said.observed(), kept.rev)).collect()
    }

    fn load(&mut self, file: &CacheFile, rev: Rev) -> rusqlite::Result<()> {
        for Row { of, value, read_at_ms } in file::load::<K>(file)? {
            let said = Said { value, current: false, as_of_ms: read_at_ms, read_at_ms };
            self.kept.insert(of, Kept { said, rev });
        }
        Ok(())
    }
}

impl Tables {
    /// Each kind's table, the one list of every kind the store keeps.
    fn each(&mut self) -> [&mut dyn AnyTable; 2] {
        let Tables { workspace_lines, workspace_changes } = self;
        [workspace_lines, workspace_changes]
    }

    /// Every observation sent at `level`, of every kind.
    fn listed(&mut self, level: &Level) -> Vec<Observation> {
        self.each().iter().flat_map(|table| table.listed(level)).collect()
    }

    /// The form of each kind's table in the cache file.
    pub(super) fn forms() -> Vec<Form> {
        Tables::default().each().iter().map(|table| table.form()).collect()
    }
}

impl Observations {
    /// The store kept in `file`, whose tables [`prepare`](super::prepare) made, with each observation it kept from
    /// before as of when it was read, under one revision newer than any a client holds, since a client of the hub
    /// before can hold any revision that hub gave. Where there's no revision to give, as when the database fails, it
    /// starts with none of them, as the log says, and each is not known until it's read again.
    pub(crate) fn open(file: Arc<CacheFile>, store: &Store) -> Self {
        let mut tables = Tables::default();
        match store.next_rev() {
            Ok(rev) => {
                for table in tables.each() {
                    if let Err(error) = table.load(&file, rev) {
                        warn!(table = table.form().table, "reading observations from the cache's file: {error}");
                    }
                }
            }
            Err(error) => {
                warn!("leaving the observations in the cache's file unread, with no revision for them: {error}");
            }
        }
        Self { changing: AsyncMutex::new(()), tables: Mutex::new(tables), file }
    }

    /// Lists the records along with `list`, and every observation the hub sends at its own level, under the lock a
    /// change takes, so the list's revision takes in none it lacks.
    pub(crate) async fn listed_with<T>(&self, list: impl Future<Output = T>) -> (T, Vec<Observation>) {
        let _changing = self.changing.lock().await;
        let listed = list.await;
        (listed, self.tables.lock().listed(&Level::Hub))
    }

    /// Every observation of `level`, which isn't the hub's, with the hub's newest revision, `rev_now`, read under the
    /// lock every change takes, so the revision covers no change the list lacks. A record that changes meanwhile moves
    /// the revision too, which costs nothing, since a client compares only this level's copies with the list.
    pub(crate) async fn listed_at(
        &self,
        level: &Level,
        rev_now: impl Future<Output = Result<Rev, ApiError>>,
    ) -> Result<(Vec<Observation>, Rev), ApiError> {
        let _changing = self.changing.lock().await;
        let rev = rev_now.await?;
        Ok((self.tables.lock().listed(level), rev))
    }
}
